use crate::dns::{Fqdn, Requests, ZoneId};
use anyhow::Result;
use cloudflare::endpoints::dns::{DnsContent, DnsRecord};
use cloudflare::framework::async_api::Client;
use local_ip_address as local;
use public_ip::{http, Version};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;

type IpPair = (Option<IpAddr>, Option<IpAddr>);

pub trait DynDns {
	fn update(
		&self,
		client: Arc<Client>,
		rec: Option<DnsRecord>,
		name: Fqdn,
		id: ZoneId,
	) -> Option<JoinHandle<Result<()>>>;
}

impl DynDns for Option<IpAddr> {
	fn update(
		&self,
		client: Arc<Client>,
		rec: Option<DnsRecord>,
		name: Fqdn,
		id: ZoneId,
	) -> Option<JoinHandle<Result<()>>> {
		if let Some(ip) = *self {
			if let Some(rec) = rec {
				match rec.content {
					DnsContent::A { content: _ }
					| DnsContent::AAAA { content: _ } => Some(tokio::spawn(async move {
						if let Some(req) = rec.update_request(ip, &id) {
							client.request(&req).await?;
						}
						Ok(())
					})),
					_ => None,
				}
			} else {
				Some(tokio::spawn(async move {
					client
						.request(&DnsRecord::create_request(ip, &name, &id))
						.await?;
					Ok(())
				}))
			}
		} else {
			rec.map(|rec| {
				tokio::spawn(async move {
					client.request(&rec.delete_request(&id)).await?;
					Ok(())
				})
			})
		}
	}
}

pub async fn get_ips(loc: bool) -> Result<IpPair> {
	let (ipv4, ipv6) = if loc {
		(local::local_ip().ok(), local::local_ipv6().ok())
	} else {
		tokio::join!(
			public_ip::addr_with(http::ALL, Version::V4),
			public_ip::addr_with(public_ip::ALL, Version::V6)
		)
	};

	if (None, None) == (ipv4, ipv6) {
		Err(anyhow::anyhow!(
			"Could not determine your current public IP address."
		))?
	}

	if let Some(ip) = ipv4 {
		log::info!("{}", ip);
	}
	if let Some(ip) = ipv6 {
		log::info!("{}", ip);
	};
	Ok((ipv4, ipv6))
}
