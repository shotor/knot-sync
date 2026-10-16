use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::StreamExt;
use hickory_client::client::{Client, ClientHandle};
use hickory_client::proto::dnssec::rdata::tsig::TsigAlgorithm;
use hickory_client::proto::dnssec::tsig::TSigner;
use hickory_client::proto::op::{Message, MessageType, OpCode, Query, ResponseCode, UpdateMessage};
use hickory_client::proto::rr::{DNSClass, Name, Record, RecordType};
use hickory_client::proto::runtime::TokioRuntimeProvider;
use hickory_client::proto::tcp::TcpClientStream;
use hickory_client::proto::xfer::{DnsHandle, DnsResponse, FirstAnswer};

use crate::config::Config;
use crate::records::{Changes, Rrset, Rrsets};
use crate::utils::secret::read_secret;

const TSIG_FUDGE: u16 = 300;
const TIMEOUT: Duration = Duration::from_secs(10);

/// A TCP connection to the Knot root; every message on it is signed with the TSIG key.
pub struct Knot {
  client: Client,
}

impl Knot {
  pub async fn connect(config: &Config) -> anyhow::Result<Self> {
    let address = tokio::net::lookup_host(&config.dns_server)
      .await
      .with_context(|| format!("resolve {}", config.dns_server))?
      .next()
      .with_context(|| format!("{} has no address", config.dns_server))?;

    let secret = read_secret(&config.tsig_key_file).await?;
    let key = STANDARD
      .decode(secret)
      .context("the TSIG key is not base64")?;

    let signer = TSigner::new(
      key,
      TsigAlgorithm::HmacSha256,
      Name::from_ascii(&config.tsig_key_name)?,
      TSIG_FUDGE,
    )
    .context("TSIG signer")?;

    let (stream, sender) =
      TcpClientStream::new(address, None, Some(TIMEOUT), TokioRuntimeProvider::new());

    let (client, background) =
      Client::with_timeout(stream, sender, TIMEOUT, Some(Arc::new(signer)))
        .await
        .with_context(|| format!("connect {address}"))?;

    tokio::spawn(background);

    Ok(Self { client })
  }

  /// Every RRset of the zone as served, by zone transfer.
  pub async fn present(&mut self, origin: &Name) -> anyhow::Result<Rrsets> {
    let mut transfer = self.client.zone_transfer(origin.clone(), None);
    let mut present = Rrsets::new();

    while let Some(response) = transfer.next().await {
      let response = response.with_context(|| format!("zone transfer of {origin}"))?;

      check(&response, &format!("zone transfer of {origin}"))?;

      for record in response.answers() {
        present
          .entry((record.name().to_lowercase(), record.record_type()))
          .or_insert_with(|| Rrset {
            ttl: record.ttl(),
            data: Vec::new(),
          })
          .push(record.data().clone());
      }
    }

    Ok(present)
  }

  /// The whole change as one update message, which the server applies all or nothing: additions
  /// first, so an RRset being replaced is never empty in between.
  pub async fn apply(&mut self, origin: &Name, changes: &Changes) -> anyhow::Result<()> {
    let mut message = update_message(origin);

    for (name, _, ttl, data) in &changes.add {
      for rdata in data {
        message.add_update(Record::from_rdata(name.clone(), *ttl, rdata.clone()));
      }
    }

    for (name, _, data) in &changes.remove {
      for rdata in data {
        let mut record = Record::from_rdata(name.clone(), 0, rdata.clone());
        record.set_dns_class(DNSClass::NONE);
        message.add_update(record);
      }
    }

    for (name, record_type) in &changes.delete {
      let mut record = Record::update0(name.clone(), 0, *record_type).into_record_of_rdata();
      record.set_dns_class(DNSClass::ANY);
      message.add_update(record);
    }

    let response = self
      .client
      .send(message)
      .first_answer()
      .await
      .with_context(|| format!("update {origin}"))?;

    check(&response, &format!("update {origin}"))
  }
}

/// The client gives the message its id when it sends it.
fn update_message(origin: &Name) -> Message {
  let mut zone = Query::new();
  zone
    .set_name(origin.clone())
    .set_query_class(DNSClass::IN)
    .set_query_type(RecordType::SOA);

  let mut message = Message::new();
  message
    .set_message_type(MessageType::Query)
    .set_op_code(OpCode::Update)
    .set_recursion_desired(false);
  message.add_zone(zone);

  message
}

pub fn fqdn(name: &str) -> anyhow::Result<Name> {
  let mut name = Name::from_ascii(name)?;
  name.set_fqdn(true);

  Ok(name.to_lowercase())
}

fn check(response: &DnsResponse, what: &str) -> anyhow::Result<()> {
  let code = response.response_code();

  if code != ResponseCode::NoError {
    bail!("{what}: {code}");
  }

  Ok(())
}
