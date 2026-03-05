use reqwest::{Client, header};

use crate::config::Config;
use crate::models::{DocResponse, ListResponse};

pub struct ErpNextClient {
    http: Client,
    base_url: String,
}

impl ErpNextClient {
    pub fn new(config: &Config) -> crate::Result<Self> {
        let mut headers = header::HeaderMap::new();
        let auth_value = format!("token {}:{}", config.api_key, config.api_secret);
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&auth_value)
                .map_err(|e| crate::Error::Config(e.to_string()))?,
        );

        let http = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(crate::Error::Http)?;

        Ok(Self {
            http,
            base_url: config.erpnext_url.trim_end_matches('/').to_string(),
        })
    }

    /// Fetch a list of documents.
    pub async fn list(
        &self,
        doctype: &str,
        filters: Option<&serde_json::Value>,
        fields: Option<&[&str]>,
        limit: Option<u32>,
    ) -> crate::Result<Vec<serde_json::Value>> {
        let url = format!("{}/api/resource/{doctype}", self.base_url);
        let mut req = self.http.get(&url);

        if let Some(f) = filters {
            req = req.query(&[("filters", &serde_json::to_string(f)?)]);
        }
        if let Some(f) = fields {
            req = req.query(&[("fields", &serde_json::to_string(f)?)]);
        }
        if let Some(l) = limit {
            req = req.query(&[("limit_page_length", l)]);
        }

        let resp: ListResponse = req.send().await?.error_for_status()?.json().await?;
        Ok(resp.data)
    }

    /// Fetch a single document.
    pub async fn get(&self, doctype: &str, name: &str) -> crate::Result<serde_json::Value> {
        let url = format!("{}/api/resource/{doctype}/{name}", self.base_url);
        let resp: DocResponse = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data)
    }

    /// Create a document.
    pub async fn create(
        &self,
        doctype: &str,
        data: &serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        let url = format!("{}/api/resource/{doctype}", self.base_url);
        let resp: DocResponse = self
            .http
            .post(&url)
            .json(data)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data)
    }

    /// Update a document.
    pub async fn update(
        &self,
        doctype: &str,
        name: &str,
        data: &serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        let url = format!("{}/api/resource/{doctype}/{name}", self.base_url);
        let resp: DocResponse = self
            .http
            .put(&url)
            .json(data)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data)
    }

    /// Call a whitelisted ERPNext method.
    pub async fn call_method(
        &self,
        method: &str,
        args: Option<&serde_json::Value>,
    ) -> crate::Result<serde_json::Value> {
        let url = format!("{}/api/method/{method}", self.base_url);
        let mut req = self.http.post(&url);
        if let Some(a) = args {
            req = req.json(a);
        }
        let resp: serde_json::Value = req.send().await?.error_for_status()?.json().await?;
        Ok(resp)
    }
}
