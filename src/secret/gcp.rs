use std::path::Path;

use futures::{Stream, TryStreamExt};
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::buckets::get::GetBucketRequest;
use google_cloud_storage::http::buckets::Bucket;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use serde::Deserialize;

use crate::{IntoSecretStorage, SecretError, SecretStorage};

#[derive(Debug, thiserror::Error)]
pub enum GoogleCloudError {
    #[error("failed to create client: {0}")]
    CreatingClient(google_cloud_auth::error::Error),
    #[error("failed to fetch specified bucket: {0}")]
    FetchingBucket(google_cloud_storage::http::Error),
    #[error("failed to download secret: {0}")]
    DownloadingSecret(google_cloud_storage::http::Error),
    #[error("failed to upload secret: {0}")]
    UploadingSecret(google_cloud_storage::http::Error),
}

impl SecretError for GoogleCloudError {}

pub struct GoogleCloudStorage {
    client: Client,
    bucket: Bucket,
}

#[derive(Deserialize, Debug)]
pub struct GoogleCloudConfig {
    bucket: String,
}

impl IntoSecretStorage for GoogleCloudConfig {
    type Error = GoogleCloudError;
    type Impl = GoogleCloudStorage;

    async fn build(self) -> Result<Self::Impl, Self::Error> {
        let config = ClientConfig::default()
            .with_auth()
            .await
            .map_err(GoogleCloudError::CreatingClient)?;

        let client = Client::new(config);
        let bucket = client
            .get_bucket(&GetBucketRequest {
                bucket: self.bucket,
                ..Default::default()
            })
            .await
            .map_err(GoogleCloudError::FetchingBucket)?;

        Ok(Self::Impl { client, bucket })
    }
}

impl SecretStorage for GoogleCloudStorage {
    type Error = GoogleCloudError;

    async fn read_stream(
        &self,
        p: &Path,
    ) -> Result<impl Stream<Item = Result<bytes::Bytes, Self::Error>> + Send + 'static, Self::Error>
    {
        let range = Range::default();
        let object = self
            .client
            .download_streamed_object(
                &GetObjectRequest {
                    bucket: self.bucket.name.clone(),
                    object: p.to_string_lossy().to_string(),
                    ..Default::default()
                },
                &range,
            )
            .await
            .map_err(GoogleCloudError::DownloadingSecret)?;

        Ok(object.map_err(GoogleCloudError::DownloadingSecret))
    }

    async fn write<R>(&self, p: &Path, new_encrypted_content: R) -> Result<(), Self::Error>
    where
        // R: tokio::io::AsyncRead + Send + Sync + Unpin + Clone + 'a + 'b,
        R: Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + Sync + Unpin + 'static,
    {
        let r = new_encrypted_content;
        // TODO: need to sort out reader stream typing here
        // let encrypted_reader = new_encrypted_content
        //     .map(|r| r.map_err(|e| std::io::Error::new(ErrorKind::Other, format!("{e}"))))
        //     .into_async_read();
        // let encrypted_reader = Box::pin(new_encrypted_content.compat());
        // let encrypted_reader = ReaderStream::new(r);
        let name = p.to_string_lossy().to_string();
        self.client
            .upload_streamed_object(
                &UploadObjectRequest {
                    bucket: self.bucket.name.clone(),
                    ..Default::default()
                },
                r,
                &UploadType::Simple(Media::new(name)),
            )
            .await
            .map_err(GoogleCloudError::UploadingSecret)?;

        Ok(())
    }
}
