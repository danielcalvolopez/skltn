/// This module handles authentication logic.
///
/// It provides token-based auth with refresh capabilities.

use std::time::Duration;

/// Validates a JWT token and returns the decoded claims.
///
/// # Arguments
/// * `token` - The JWT string to validate
/// * `secret` - The signing secret
///
/// # Returns
/// The decoded claims if valid, or an error.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    // First decode the header
    let header = decode_header(token)?;
    // Then verify the signature
    let claims = verify_signature(token, secret, &header)?;
    // Check expiration
    if claims.exp < current_timestamp() {
        return Err(AuthError::Expired);
    }
    Ok(claims)
}

/// Represents decoded JWT claims.
pub struct Claims {
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
}

/// Authentication errors.
#[derive(Debug)]
pub enum AuthError {
    /// The token has expired.
    Expired,
    /// The signature is invalid.
    InvalidSignature,
    /// The token format is malformed.
    Malformed(String),
}
