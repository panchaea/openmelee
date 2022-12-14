use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use figment::{providers::Env, providers::Serialized, Figment};
use once_cell::sync::Lazy;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tera::Tera;
use url::Url;

pub mod auth;
pub mod game;
pub mod models;

pub const LATEST_SLIPPI_CLIENT_VERSION: &str = "2.5.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub webserver_address: IpAddr,
    pub webserver_port: u16,
    pub matchmaking_server_address: Ipv4Addr,
    pub matchmaking_port: u16,
    pub matchmaking_max_peers: u64,
    pub database_url: String,
    pub database_max_connections: u32,
    pub public_url: Option<Url>,
    pub jwt_secret_path: Option<String>,
    pub cookie_secret_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            webserver_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            webserver_port: 5000,
            matchmaking_server_address: Ipv4Addr::LOCALHOST,
            matchmaking_port: 43113,
            matchmaking_max_peers: 1024,
            database_url: "openmelee.sqlite".to_string(),
            database_max_connections: 10,
            public_url: None,
            jwt_secret_path: None,
            cookie_secret_path: Some("openmelee-cookie.key".to_string()),
        }
    }
}

impl Config {
    pub fn format_webserver_address(self) -> String {
        format!("http://{}:{}/", self.webserver_address, self.webserver_port)
    }

    pub fn format_matchmaking_server_address(self) -> String {
        format!(
            "udp://{}:{}",
            self.matchmaking_server_address, self.matchmaking_port
        )
    }

    pub fn format_matchmaking_host(self) -> String {
        self.clone()
            .public_url
            .and_then(|public_url| Some(public_url.host_str().unwrap().to_string()))
            .unwrap_or("localhost".to_string())
    }

    pub fn format_user_discovery_url(self) -> String {
        let url = self
            .clone()
            .public_url
            .and_then(|public_url| Some(public_url.to_string()))
            .unwrap_or(Config::format_webserver_address(self));

        format!("{}user", url)
    }

    pub fn can_set_secure_cookie(self) -> bool {
        self.public_url.is_some() && self.public_url.unwrap().scheme() == "https"
    }
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    Figment::from(Serialized::defaults(Config::default()))
        .merge(Env::prefixed("OPENMELEE_"))
        .extract()
        .unwrap()
});

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Asset;

pub static TEMPLATES: Lazy<Tera> = Lazy::new(|| {
    let templates = Asset::iter()
        .into_iter()
        .filter(|asset_path| asset_path.ends_with(".tera"))
        .map(move |asset_path| {
            let _asset_path = asset_path.clone();
            let asset = Asset::get(&_asset_path).unwrap();
            let contents = std::str::from_utf8(asset.data.as_ref()).unwrap();

            (
                std::path::Path::new(&asset_path.to_string())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
                contents.to_string(),
            )
        });

    let mut tera = Tera::new("assets/templates/*.tera").expect("Failed to read templates");

    tera.add_raw_templates(templates)
        .expect("Failed to parse templates");

    tera
});

pub async fn init_pool(config: Config) -> SqlitePool {
    let connection_options = SqliteConnectOptions::from_str(&config.database_url.clone())
        .expect("Failed to connect to database")
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect_with(connection_options)
        .await
        .expect("Failed to initialize database pool")
}

pub async fn run_migrations(pool: &SqlitePool) {
    match sqlx::migrate!().run(pool).await {
        Ok(_) => (),
        _ => panic!("Failed to run migrations, exiting."),
    }
}

#[cfg(test)]
mod test {
    use url::Url;

    use crate::Config;

    #[test]
    fn test_format_user_discovery_url_without_public_url() {
        let mut config = Config::default();
        config.webserver_port = 5001;
        assert_eq!(
            config.format_user_discovery_url(),
            "http://127.0.0.1:5001/user"
        );
    }

    #[test]
    fn test_format_user_discovery_url_with_public_url() {
        let mut config = Config::default();
        config.public_url = Some(Url::try_from("https://example.org").unwrap());
        assert_eq!(
            config.format_user_discovery_url(),
            "https://example.org/user"
        );
    }

    #[test]
    fn test_format_matchmaking_host_without_public_url() {
        let config = Config::default();
        assert_eq!(config.format_matchmaking_host(), "localhost");
    }

    #[test]
    fn test_format_matchmaking_host_with_public_url() {
        let mut config = Config::default();
        config.public_url = Some(Url::try_from("https://example.org").unwrap());
        assert_eq!(config.format_matchmaking_host(), "example.org");
    }
}
