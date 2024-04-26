// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::handlers::committer::start_tx_checkpoint_commit_task;
use crate::models::display::StoredDisplay;
use async_trait::async_trait;
use itertools::Itertools;
use mysten_metrics::{get_metrics, spawn_monitored_task};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use sui_rest_api::CheckpointData;
use sui_rest_api::CheckpointTransaction;
use sui_types::base_types::ObjectRef;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents};
use sui_types::object::Object;

use tokio::sync::watch;

use std::collections::hash_map::Entry;
use std::collections::HashSet;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Owner;
use sui_types::transaction::TransactionDataAPI;
use tap::tap::TapFallible;
use tracing::{error, info};

use sui_types::base_types::ObjectID;

use crate::errors::IndexerError;
use crate::framework::interface::Handler;
use crate::metrics::IndexerMetrics;

use crate::store::IndexerStore;
use crate::types::{
    IndexedCheckpoint, IndexedDeletedObject, IndexedEvent, IndexedObject, IndexedPackage, IndexedTransaction, IndexerResult, TransactionKind, TxIndex
};

use super::tx_processor::TxChangesProcessor;
use super::CheckpointDataToCommit;
use super::TransactionObjectChangesToCommit;

const CHECKPOINT_QUEUE_SIZE: usize = 100;

pub async fn new_handlers<S>(
    state: S,
    metrics: IndexerMetrics,
) -> Result<CheckpointHandler, IndexerError>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    let checkpoint_queue_size = std::env::var("CHECKPOINT_QUEUE_SIZE")
        .unwrap_or(CHECKPOINT_QUEUE_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    let global_metrics = get_metrics().unwrap();
    let (indexed_checkpoint_sender, indexed_checkpoint_receiver) =
        mysten_metrics::metered_channel::channel(
            checkpoint_queue_size,
            &global_metrics
                .channels
                .with_label_values(&["checkpoint_indexing"]),
        );

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    let (tx, _) = watch::channel(None);
    spawn_monitored_task!(start_tx_checkpoint_commit_task(
        state_clone,
        metrics_clone,
        indexed_checkpoint_receiver,
        tx,
    ));

    let checkpoint_handler = CheckpointHandler {
        metrics,
        indexed_checkpoint_sender,
    };

    Ok(checkpoint_handler)
}

pub struct CheckpointHandler {
    metrics: IndexerMetrics,
    indexed_checkpoint_sender: mysten_metrics::metered_channel::Sender<CheckpointDataToCommit>,
}

#[async_trait]
impl Handler for CheckpointHandler {
    fn name(&self) -> &str {
        "checkpoint-handler"
    }
    async fn process_checkpoints(&mut self, checkpoints: &[CheckpointData]) -> anyhow::Result<()> {
        if checkpoints.is_empty() {
            return Ok(());
        }
        // Safe to unwrap, checked emptiness above
        let first_checkpoint_seq = checkpoints
            .first()
            .unwrap()
            .checkpoint_summary
            .sequence_number();
        let last_checkpoint_seq = checkpoints
            .last()
            .unwrap()
            .checkpoint_summary
            .sequence_number();
        info!(
            first = first_checkpoint_seq,
            last = last_checkpoint_seq,
            "Checkpoints received by CheckpointHandler"
        );

        let indexing_timer = self.metrics.checkpoint_index_latency.start_timer();
        // It's important to index packages first to populate ModuleResolver
        let packages = Self::index_packages(checkpoints, &self.metrics);

        let mut packages_per_checkpoint: HashMap<_, Vec<_>> = HashMap::new();
        for package in packages {
            packages_per_checkpoint
                .entry(package.checkpoint_sequence_number)
                .or_default()
                .push(package);
        }
        let mut tasks = vec![];
        let metrics_clone = Arc::new(self.metrics.clone());
        for checkpoint in checkpoints {
            let packages = packages_per_checkpoint
                .remove(checkpoint.checkpoint_summary.sequence_number())
                .unwrap_or_default();
            tasks.push(tokio::task::spawn(Self::index_one_checkpoint(
                checkpoint.clone(),
                metrics_clone.clone(),
                packages,
            )));
        }
        let checkpoint_data_to_commit = futures::future::join_all(tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| {
                error!(
                    "Failed to join all checkpoint indexing tasks with error: {}",
                    e.to_string()
                );
            })?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| {
                error!("Failed to index checkpoints with error: {}", e.to_string());
            })?;
        let elapsed = indexing_timer.stop_and_record();

        info!(
            first = first_checkpoint_seq,
            last = last_checkpoint_seq,
            elapsed,
            "Checkpoints indexing finished, about to sending to commit handler"
        );

        // NOTE: when the channel is full, checkpoint_sender_guard will wait until the channel has space.
        // Checkpoints are sent sequentially to stick to the order of checkpoint sequence numbers.
        for checkpoint_data in checkpoint_data_to_commit {
            let checkpoint_seq = checkpoint_data.checkpoint.sequence_number;
            self.indexed_checkpoint_sender
                .send(checkpoint_data)
                .await
                .tap_ok(|_| info!(checkpoint_seq, "Checkpoint sent to commit handler"))
                .unwrap_or_else(|e| {
                    panic!(
                        "checkpoint channel send should not fail, but got error: {:?}",
                        e
                    )
                });
        }
        Ok(())
    }
}

impl CheckpointHandler {
    pub fn new(
        metrics: IndexerMetrics,
        indexed_checkpoint_sender: mysten_metrics::metered_channel::Sender<CheckpointDataToCommit>,
    ) -> Self {
        Self {
            metrics,
            indexed_checkpoint_sender,
        }
    }

    async fn index_one_checkpoint(
        data: CheckpointData,
        metrics: Arc<IndexerMetrics>,
        packages: Vec<IndexedPackage>,
    ) -> Result<CheckpointDataToCommit, IndexerError> {
        let checkpoint_seq = data.checkpoint_summary.sequence_number;
        info!(checkpoint_seq, "Indexing checkpoint data blob");

        let object_changes: TransactionObjectChangesToCommit = Self::index_objects(data.clone(), &metrics).await?;
        
        // NULL object
        let object_history_changes = TransactionObjectChangesToCommit {
            changed_objects: vec![],
            deleted_objects: vec![],
        };

        let (checkpoint, db_transactions, db_events, db_indices, db_displays) = {
            let CheckpointData {
                transactions,
                checkpoint_summary,
                checkpoint_contents,
            } = data;

            let (db_transactions, db_events, db_indices, db_displays) = Self::index_transactions(
                transactions,
                &checkpoint_summary,
                &checkpoint_contents,
                &metrics,
            )
            .await?;

            let successful_tx_num: u64 = db_transactions.iter().map(|t| t.successful_tx_num).sum();
            (
                IndexedCheckpoint::from_sui_checkpoint(
                    &checkpoint_summary,
                    &checkpoint_contents,
                    successful_tx_num as usize,
                ),
                db_transactions,
                db_events,
                db_indices,
                db_displays,
            )
        };
                
        info!(checkpoint_seq, "Indexed one checkpoint.");
        Ok(CheckpointDataToCommit {
            checkpoint,
            transactions: db_transactions,
            events: db_events,
            tx_indices: db_indices,
            display_updates: db_displays,
            object_changes,
            object_history_changes,
            packages,
            epoch: None,
        })
    }

    async fn index_objects(
      data: CheckpointData,
      metrics: &IndexerMetrics,
    ) -> Result<TransactionObjectChangesToCommit, IndexerError> {
      let _timer = metrics.indexing_objects_latency.start_timer();
      let checkpoint_seq = data.checkpoint_summary.sequence_number;
      let deleted_objects = data
          .transactions
          .iter()
          .flat_map(|tx| {
            get_deleted_objects(&tx.effects).into_iter().map(|d| (*tx.transaction.digest(), d))
          })
          .collect::<Vec<_>>();
      let deleted_object_ids = deleted_objects
          .iter()
          .map(|o| (o.1.0, o.1.1))
          .collect::<HashSet<_>>();
      let indexed_deleted_objects = deleted_objects
          .into_iter()
          .map(|o| IndexedDeletedObject {
              object_id: o.1.0,
              object_version: o.1.1.value(),
              checkpoint_sequence_number: checkpoint_seq,
              tx_digest: o.0,
          })
          .collect();

      let (latest_objects, intermediate_versions) = get_latest_objects(data.output_objects());

      let live_objects: Vec<Object> = data
          .transactions
          .iter()
          .flat_map(|tx| {
              let CheckpointTransaction {
                  transaction: tx,
                  effects: fx,
                  ..
              } = tx;
              fx.all_changed_objects()
                  .into_iter()
                  .filter_map(|(oref, _owner, _kind)| {
                      // We don't care about objects that are deleted or updated more than once
                      if intermediate_versions.contains(&(oref.0, oref.1))
                          || deleted_object_ids.contains(&(oref.0, oref.1))
                      {
                          return None;
                      }
                      let object = latest_objects.get(&(oref.0)).unwrap_or_else(|| {
                          panic!(
                              "object {:?} not found in CheckpointData (tx_digest: {})",
                              oref.0,
                              tx.digest()
                          )
                      });
                      assert_eq!(oref.1, object.version());
                      Some(object.clone())
                  })
                  .collect::<Vec<_>>()
          })
          .collect();

      let changed_objects = live_objects
          .into_iter()
          .map(|o| IndexedObject::from_object(checkpoint_seq, o, None))
          .collect::<Vec<_>>();

      Ok(TransactionObjectChangesToCommit {
          changed_objects,
          deleted_objects: indexed_deleted_objects,
      })
    }

    async fn index_transactions(
        transactions: Vec<CheckpointTransaction>,
        checkpoint_summary: &CertifiedCheckpointSummary,
        checkpoint_contents: &CheckpointContents,
        metrics: &IndexerMetrics,
    ) -> IndexerResult<(
        Vec<IndexedTransaction>,
        Vec<IndexedEvent>,
        Vec<TxIndex>,
        BTreeMap<String, StoredDisplay>,
    )> {
        let checkpoint_seq = checkpoint_summary.sequence_number();

        let mut tx_seq_num_iter = checkpoint_contents
            .enumerate_transactions(checkpoint_summary)
            .map(|(seq, execution_digest)| (execution_digest.transaction, seq));

        if checkpoint_contents.size() != transactions.len() {
            return Err(IndexerError::FullNodeReadingError(format!(
                "CheckpointContents has different size {} compared to Transactions {} for checkpoint {}",
                checkpoint_contents.size(),
                transactions.len(),
                checkpoint_seq
            )));
        }

        let mut db_transactions = Vec::new();
        let mut db_events = Vec::new();
        let mut db_displays = BTreeMap::new();
        let mut db_indices = Vec::new();

        for tx in transactions {
            let CheckpointTransaction {
                transaction: sender_signed_data,
                effects: fx,
                events,
                input_objects,
                output_objects,
            } = tx;
            // Unwrap safe - we checked they have equal length above
            let (tx_digest, tx_sequence_number) = tx_seq_num_iter.next().unwrap();
            if tx_digest != *sender_signed_data.digest() {
                return Err(IndexerError::FullNodeReadingError(format!(
                    "Transactions has different ordering from CheckpointContents, for checkpoint {}, Mismatch found at {} v.s. {}",
                    checkpoint_seq, tx_digest, sender_signed_data.digest()
                )));
            }
            let tx = sender_signed_data.transaction_data();
            let events = events
                .as_ref()
                .map(|events| events.data.clone())
                .unwrap_or_default();

            let transaction_kind = if tx.is_system_tx() {
                TransactionKind::SystemTransaction
            } else {
                TransactionKind::ProgrammableTransaction
            };

            db_events.extend(events.iter().enumerate().map(|(idx, event)| {
                IndexedEvent::from_event(
                    tx_sequence_number,
                    idx as u64,
                    *checkpoint_seq,
                    tx_digest,
                    event,
                    checkpoint_summary.timestamp_ms,
                )
            }));

            db_displays.extend(
                events
                    .iter()
                    .flat_map(StoredDisplay::try_from_event)
                    .map(|display| (display.object_type.clone(), display)),
            );

            let objects = input_objects
                .iter()
                .chain(output_objects.iter())
                .collect::<Vec<_>>();

            let (balance_change, object_changes) =
                TxChangesProcessor::new(&objects, metrics.clone())
                    .get_changes(tx, &fx, &tx_digest)
                    .await?;

            let db_txn = IndexedTransaction {
                tx_sequence_number,
                tx_digest,
                checkpoint_sequence_number: *checkpoint_summary.sequence_number(),
                timestamp_ms: checkpoint_summary.timestamp_ms,
                sender_signed_data: sender_signed_data.data().clone(),
                effects: fx.clone(),
                object_changes,
                balance_change,
                events,
                transaction_kind,
                successful_tx_num: if fx.status().is_ok() {
                    tx.kind().tx_count() as u64
                } else {
                    0
                },
            };

            db_transactions.push(db_txn);

            // Input Objects
            let input_objects = tx
                .input_objects()
                .expect("committed txns have been validated")
                .into_iter()
                .map(|obj_kind| obj_kind.object_id())
                .collect::<Vec<_>>();

            // Changed Objects
            let changed_objects = fx
                .all_changed_objects()
                .into_iter()
                .map(|(object_ref, _owner, _write_kind)| object_ref.0)
                .collect::<Vec<_>>();

            // Payers
            let payers = vec![tx.gas_owner()];

            // Senders
            let senders = vec![tx.sender()];

            // Recipients
            let recipients = fx
                .all_changed_objects()
                .into_iter()
                .filter_map(|(_object_ref, owner, _write_kind)| match owner {
                    Owner::AddressOwner(address) => Some(address),
                    _ => None,
                })
                .unique()
                .collect::<Vec<_>>();

            // Move Calls
            let move_calls = tx
                .move_calls()
                .iter()
                .map(|(p, m, f)| (*<&ObjectID>::clone(p), m.to_string(), f.to_string()))
                .collect();

            db_indices.push(TxIndex {
                tx_sequence_number,
                transaction_digest: tx_digest,
                checkpoint_sequence_number: *checkpoint_seq,
                input_objects,
                changed_objects,
                senders,
                payers,
                recipients,
                move_calls,
            });
        }
        Ok((db_transactions, db_events, db_indices, db_displays))
    }

    fn index_packages(
        checkpoint_data: &[CheckpointData],
        metrics: &IndexerMetrics,
    ) -> Vec<IndexedPackage> {
        let _timer = metrics.indexing_packages_latency.start_timer();
        checkpoint_data
            .iter()
            .flat_map(|data| {
                let checkpoint_sequence_number = data.checkpoint_summary.sequence_number;
                data.output_objects()
                    .iter()
                    .filter_map(|o| {
                        if let sui_types::object::Data::Package(p) = &o.data {
                            Some(IndexedPackage {
                                package_id: o.id(),
                                move_package: p.clone(),
                                checkpoint_sequence_number,
                            })
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

pub fn get_deleted_objects(effects: &TransactionEffects) -> Vec<ObjectRef> {
    let deleted = effects.deleted().into_iter();
    let wrapped = effects.wrapped().into_iter();
    let unwrapped_then_deleted = effects.unwrapped_then_deleted().into_iter();
    deleted
        .chain(wrapped)
        .chain(unwrapped_then_deleted)
        .collect::<Vec<_>>()
}

pub fn get_latest_objects(
    objects: Vec<&Object>,
) -> (
    HashMap<ObjectID, Object>,
    HashSet<(ObjectID, SequenceNumber)>,
) {
    let mut latest_objects = HashMap::new();
    let mut discarded_versions = HashSet::new();
    for object in objects {
        match latest_objects.entry(object.id()) {
            Entry::Vacant(e) => {
                e.insert(object.clone());
            }
            Entry::Occupied(mut e) => {
                if object.version() > e.get().version() {
                    discarded_versions.insert((e.get().id(), e.get().version()));
                    e.insert(object.clone());
                }
            }
        }
    }
    (latest_objects, discarded_versions)
}
