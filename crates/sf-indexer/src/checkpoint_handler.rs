use eyre::{Result, Report};
use jsonrpsee::http_client::{HttpClient};
use futures::future::join_all;
use sui_indexer::{
  store::CheckpointData, utils::multi_get_full_transactions,
  handlers::checkpoint_handler::{get_object_changes, fetch_changed_objects},
};
use sui_json_rpc::api::ReadApiClient;
use sui_json_rpc_types::Checkpoint;

const MULTI_GET_CHUNK_SIZE: usize = 50;

type CheckpointSequenceNumber = u64;

pub struct CheckpointHandler {
  http_client: HttpClient,
}

impl CheckpointHandler {
  pub fn new(
    http_client: HttpClient,
  ) -> Self {
    Self {
      http_client,
    }
  }

  /// Download all the data we need for one checkpoint.
  pub async fn download_checkpoint_data(&self, seq: CheckpointSequenceNumber) -> Result<CheckpointData> {
    let checkpoint = self.get_checkpoint(seq).await?;
    let transactions = join_all(checkpoint.transactions.chunks(MULTI_GET_CHUNK_SIZE)
    .map(|digests| multi_get_full_transactions(self.http_client.clone(), digests.to_vec())))
    .await
    .into_iter()
    .try_fold(vec![], |mut acc, chunk| {
      acc.extend(chunk?);
      Ok::<_, Report>(acc)
    })?;

    let object_changes = transactions
    .iter()
    .flat_map(|tx| get_object_changes(&tx.effects))
    .collect::<Vec<_>>();
    let changed_objects = fetch_changed_objects(self.http_client.clone(), object_changes).await?;

    Ok(CheckpointData {
      checkpoint,
      transactions,
      changed_objects,
    })
  }

  async fn get_checkpoint(&self, seq: CheckpointSequenceNumber) -> Result<Checkpoint> {
    let mut checkpoint = Err(Report::msg("Empty Error"));

    while checkpoint.is_err() {
      // sleep for 0.1 second and retry if latest checkpoint is not available yet
      tokio::time::sleep(std::time::Duration::from_millis(100)).await;
      
      checkpoint = self.http_client
      .get_checkpoint(seq.into())
      .await
      .map_err(|e| {
        Report::msg(format!("Failed to get checkpoint with sequence number {} and error {:?}", seq, e))
      })
    }

    Ok(checkpoint?)
  }
}
