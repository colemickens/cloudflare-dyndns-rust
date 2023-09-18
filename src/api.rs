use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use cloudflare::framework::{
	async_api::Client, auth::Credentials, Environment, HttpApiClientConfig,
};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
pub struct Cli {
	/// Comma separated DNS records to update with the host's public IP
	#[clap(
		long,
		short,
		env = "CLOUDFLARE_RECORDS",
		value_name = "RECORDS",
		value_delimiter(',')
	)]
	pub records: Vec<String>,
	/// recommended: The CloudFlare API token to authenticate with
	#[clap(
		long,
		short,
		env = "CLOUDFLARE_APITOKEN",
		hide_env_values = true,
		value_name = "TOKEN",
		required_unless_present_all(["key", "email"])
	)]
	/// deprecated: The CloudFlare API key to authenticate with, also requires email
	pub token: Option<String>,
	#[clap(
		long,
		short,
		env = "CLOUDFLARE_APIKEY",
		hide_env_values = true,
		value_name = "KEY",
		required_unless_present("token"),
		requires("email")
	)]
	/// deprecated: The CloudFlare email to authenticate with, also requires API key
	pub key: Option<String>,
	#[clap(
		long,
		short,
		env = "CLOUDFLARE_EMAIL",
		value_name = "EMAIL",
		required_unless_present("token"),
		requires("key")
	)]
	pub email: Option<String>,

	#[clap(flatten)]
	pub verbose: Verbosity<InfoLevel>,
}

pub fn get_client(cli: &Cli) -> Result<Client> {
	let credentials: Credentials = if let Some(token) = cli.token.clone() {
		Ok(Credentials::UserAuthToken { token })
	} else if let (Some(key), Some(email)) =
		(cli.key.clone(), cli.email.clone())
	{
		log::warn!("API Key & Email combo is deprecated. Please switch to using an API token");
		Ok(Credentials::UserAuthKey { email, key })
	} else {
		Err(anyhow::anyhow!("No valid credentials passed"))
	}?;

	Client::new(
		credentials,
		HttpApiClientConfig::default(),
		Environment::Production,
	)
}
