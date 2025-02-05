use crate::api::Cli;
use anyhow::Result;
use cloudflare::{
	endpoints::{
		dns::{
			CreateDnsRecord, CreateDnsRecordParams, DeleteDnsRecord,
			DnsContent, DnsRecord, ListDnsRecords, ListDnsRecordsParams, Meta,
			UpdateDnsRecord, UpdateDnsRecordParams,
		},
		zone::{ListZones, ListZonesParams},
	},
	framework::{async_api::Client, SearchMatch},
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

pub type Fqdn = Arc<str>;
pub type ZoneId = Arc<str>;
type Record = (Fqdn, Option<ZoneId>, Option<DnsRecord>, Option<DnsRecord>);

pub trait Clone_ {
	fn clone(&self) -> Self;
}

trait Info {
	fn get_ip(&self) -> Option<IpAddr>;
	fn get_type(&self) -> &str;
}

pub trait Requests {
	fn update_request<'a>(
		&'a self,
		ip: IpAddr,
		id: &'a str,
	) -> Option<UpdateDnsRecord<'a>>;
	fn create_request<'a>(
		ip: IpAddr,
		name: &'a str,
		id: &'a str,
	) -> CreateDnsRecord<'a>;
	fn delete_request<'a>(
		&'a self,
		id: &'a str,
	) -> DeleteDnsRecord<'a>;
}
impl Clone_ for DnsRecord {
	fn clone(&self) -> Self {
		Self {
			name: self.name.to_owned(),
			meta: Meta {
				auto_added: self.meta.auto_added,
			},
			ttl: self.ttl,
			modified_on: self.modified_on,
			created_on: self.created_on,
			proxiable: self.proxiable,
			proxied: self.proxied,
			content: self.content.to_owned(),
			id: self.id.to_owned(),
		}
	}
}

impl Info for DnsContent {
	fn get_ip(&self) -> Option<IpAddr> {
		match self {
			DnsContent::A { content: ip } => Some(IpAddr::V4(*ip)),
			DnsContent::AAAA { content: ip } => Some(IpAddr::V6(*ip)),
			_ => None,
		}
	}
	fn get_type(&self) -> &str {
		match self {
			DnsContent::A { content: _ } => "A",
			DnsContent::AAAA { content: _ } => "AAAA",
			DnsContent::CNAME { content: _ } => "CNAME",
			DnsContent::NS { content: _ } => "NS",
			DnsContent::MX {
				content: _,
				priority: _,
			} => "MX",
			DnsContent::TXT { content: _ } => "TXT",
			DnsContent::SRV { content: _ } => "SRV",
		}
	}
}

impl Requests for DnsRecord {
	fn update_request<'a>(
		&'a self,
		ip: IpAddr,
		id: &'a str,
	) -> Option<UpdateDnsRecord<'a>> {
		let current_ip = self.content.get_ip()?;

		if ip == current_ip {
			log::info!(
				"skipping {} record `{}`; already up to date",
				self.content.get_type(),
				self.name
			);
			return None;
		}

		let content = match (self.content.clone(), ip) {
			(DnsContent::A { content: _ }, IpAddr::V4(ip)) => {
				Some(DnsContent::A { content: ip })
			}
			(DnsContent::AAAA { content: _ }, IpAddr::V6(ip)) => {
				Some(DnsContent::AAAA { content: ip })
			}
			_ => None,
		}?;

		log::info!("request: {} ({} → {})\n", self.name, current_ip, ip);
		Some(UpdateDnsRecord {
			zone_identifier: id,
			identifier: &self.id,
			params: UpdateDnsRecordParams {
				name: &self.name,
				ttl: self.ttl.into(),
				proxied: self.proxied.into(),
				content,
			},
		})
	}
	fn create_request<'a>(
		ip: IpAddr,
		name: &'a str,
		id: &'a str,
	) -> CreateDnsRecord<'a> {
		log::info!("request: {} → {}\n", name, ip);
		CreateDnsRecord {
			zone_identifier: id,
			params: CreateDnsRecordParams {
				name,
				ttl: Some(1),
				priority: None,
				proxied: Some(false),
				content: match ip {
					IpAddr::V4(ip) => DnsContent::A { content: ip },
					IpAddr::V6(ip) => DnsContent::AAAA { content: ip },
				},
			},
		}
	}

	fn delete_request<'a>(
		&'a self,
		id: &'a str,
	) -> DeleteDnsRecord<'a> {
		log::info!(
			"deleting {} record: {}\n",
			self.content.get_type(),
			self.name
		);
		DeleteDnsRecord {
			zone_identifier: id,
			identifier: &self.id,
		}
	}
}

pub async fn get_records(
	cli: &Cli,
	client: Arc<Client>,
) -> Result<Vec<Record>> {
	let params = ListZones {
		params: ListZonesParams::default(),
	};
	let zones = client.request(&params).await?.result;

	let mut handles = Vec::with_capacity(zones.len());
	let mut records = Vec::with_capacity(zones.len() * 10);

	for zone in &zones {
		for dns_content in [
			DnsContent::A {
				content: Ipv4Addr::new(0, 0, 0, 0),
			},
			DnsContent::AAAA {
				content: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0),
			},
		] {
			let client = client.clone();
			let zone_id = zone.id.to_string();
			handles.push(tokio::spawn(async move {
				client
					.request(&ListDnsRecords {
						zone_identifier: &zone_id,
						params: ListDnsRecordsParams {
							record_type: Some(dns_content),
							search_match: Some(SearchMatch::Any),
							..Default::default()
						},
					})
					.await
			}));
		}
	}

	for handle in handles {
		records.extend(handle.await??.result)
	}

	Ok(cli
		.records
		.iter()
		.map(|r| {
			(
				r.clone(),
				zones
					.iter()
					.find(|z| r.contains(&z.name))
					.map(|z| Arc::from(&*z.id)),
				records
					.iter()
					.find(|rec| {
						if let DnsContent::A { content: _ } = rec.content {
							return rec.name == **r;
						}
						false
					})
					.map(|r| r.clone()),
				records
					.iter()
					.find(|rec| {
						if let DnsContent::AAAA { content: _ } = rec.content {
							return rec.name == **r;
						}
						false
					})
					.map(|r| r.clone()),
			)
		})
		.collect())
}
