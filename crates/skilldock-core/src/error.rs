#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("数据格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::Message(message.into()))
}
