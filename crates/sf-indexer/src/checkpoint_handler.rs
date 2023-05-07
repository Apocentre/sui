use std::sync::Arc;
use eyre::{Result, Report};
use jsonrpsee::http_client::{HttpClient};
use sui_core::event_handler::SubscriptionHandler;
use sui_indexer::{store::CheckpointData, models::checkpoints::Checkpoint};
use sui_json_rpc::api::ReadApiClient;

type CheckpointSequenceNumber = u64;

pub struct CheckpointHandler {
  http_client: HttpClient,
  event_handler: Arc<SubscriptionHandler>,
}

impl CheckpointHandler {
  pub fn new(
    http_client: HttpClient,
    event_handler: Arc<SubscriptionHandler>,
  ) -> Self {
    Self {
      http_client,
      event_handler,
    }
  }

  /// Download all the data we need for one checkpoint.
  async fn download_checkpoint_data(&self, seq: CheckpointSequenceNumber) -> Result<CheckpointData> {
    let latest_fn_checkpoint_seq = self
    .http_client
    .get_latest_checkpoint_sequence_number()
    .await
    .map_err(|e| {
      Report::msg(format!("Failed to get latest checkpoint sequence number and error {:?}", e))
    })?;



    todo!()
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
