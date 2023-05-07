use eyre::{Result, Report};
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use backoff::{ExponentialBackoff};
use sui_json_rpc::{CLIENT_SDK_TYPE_HEADER};
use crate::checkpoint_handler::CheckpointHandler;

pub struct FirehoseStreamer {
  pub current_checkpoint_seq: u64,
  checkpoint_handler: Option<CheckpointHandler>,
}

impl FirehoseStreamer {
  pub fn new(starting_checkpoint_seq: u64,) -> Self {
    Self {
      current_checkpoint_seq: starting_checkpoint_seq,
      checkpoint_handler: None,
    }
  }

  pub async fn start(&mut self, rpc_client_url: &str) -> Result<()> {
      // Format is FIRE INIT aptos-node <PACKAGE_VERSION> <MAJOR_VERSION> <MINOR_VERSION> <CHAIN_ID>
    println!(
      "\nFIRE INIT sui-node {} sui",
      env!("CARGO_PKG_VERSION"),
    );


    let checkpoint_handler = backoff::future::retry(ExponentialBackoff::default(), || async {
      let http_client = get_http_client(rpc_client_url)?;
      let cp = CheckpointHandler::new(http_client);

      Ok(cp)
    }).await?;

    self.checkpoint_handler = Some(checkpoint_handler);

    loop {
      self.convert_next_block().await?;
    }
  }

  pub async fn convert_next_block(&mut self) -> Result<()> {
    println!("\nFIRE BLOCK_START {}", self.current_checkpoint_seq);
    let checkpoint_handler = self.checkpoint_handler.as_ref().expect("Checkpoint handler should be created");
    let checkpoint_data = checkpoint_handler.download_checkpoint_data(self.current_checkpoint_seq).await?;

    for _onchain_txn in &checkpoint_data.transactions {

    }

    Ok(())
  }
}

fn get_http_client(rpc_client_url: &str) -> Result<HttpClient> {
  let mut headers = HeaderMap::new();
  headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("indexer"));

  HttpClientBuilder::default()
  .max_request_body_size(2 << 30)
  .max_concurrent_requests(usize::MAX)
  .set_headers(headers.clone())
  .build(rpc_client_url)
  .map_err(|e| {
    Report::msg(format!("Failed to initialize fullnode RPC client with error: {:?}", e))
  })
}
