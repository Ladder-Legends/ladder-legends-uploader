use thiserror::Error;

#[derive(Error, Debug)]
pub enum UploaderError {
    #[error("API error: {status} - {message}")]
    ApiError {
        status: u16,
        message: String,
    },

    #[error("Authentication expired")]
    AuthExpired,

    #[error("Parse error: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("File system error at {path}: {source}")]
    FileSystemError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// Allow conversion to Tauri command errors (Tauri commands return Result<T, String>)
impl From<UploaderError> for String {
    fn from(err: UploaderError) -> String {
        err.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = UploaderError::AuthExpired;
        assert_eq!(err.to_string(), "Authentication expired");
    }

    #[test]
    fn test_api_error_display() {
        let err = UploaderError::ApiError { status: 401, message: "Unauthorized".into() };
        assert_eq!(err.to_string(), "API error: 401 - Unauthorized");
    }

    #[test]
    fn test_error_from_serde() {
        let result: Result<String, UploaderError> =
            serde_json::from_str::<String>("invalid").map_err(Into::into);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Parse error"));
    }

    #[test]
    fn test_config_error() {
        let err = UploaderError::ConfigError("missing key".into());
        assert_eq!(err.to_string(), "Configuration error: missing key");
    }
}
