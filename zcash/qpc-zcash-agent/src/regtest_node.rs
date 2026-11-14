//! Authenticated, loopback-only identity attestation for the local Regtest full node.
use base64::{engine::general_purpose::STANDARD, Engine};
use http::Uri;
use std::{fmt, net::IpAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use zcash_protocol::{
    consensus::{NetworkType, NetworkUpgrade, Parameters},
    local_consensus::LocalNetwork,
};
use zeroize::{Zeroize, Zeroizing};

const RPC_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RPC_RESPONSE: usize = 1_048_576;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegtestNodeError {
    WrongNetwork,
    InvalidEndpoint,
    EndpointNotLoopback,
    InvalidCredentials,
    MalformedResponse,
    ConsensusMismatch,
    Connect,
    Timeout,
    RpcFailure,
}

pub struct FullNodeRpcConfig {
    endpoint: String,
    username: String,
    password: String,
}
impl fmt::Debug for FullNodeRpcConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FullNodeRpcConfig")
            .field("endpoint", &self.endpoint)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}
impl Drop for FullNodeRpcConfig {
    fn drop(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}
impl FullNodeRpcConfig {
    pub fn new(endpoint: &str, username: &str, password: &str) -> Result<Self, RegtestNodeError> {
        let endpoint = validate_rpc_endpoint(endpoint)?;
        if username.is_empty()
            || password.is_empty()
            || [username, password]
                .iter()
                .any(|s| s.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0))
        {
            return Err(RegtestNodeError::InvalidCredentials);
        }
        Ok(Self {
            endpoint,
            username: username.to_owned(),
            password: password.to_owned(),
        })
    }
}

pub fn validate_rpc_endpoint(endpoint: &str) -> Result<String, RegtestNodeError> {
    let uri: Uri = endpoint
        .parse()
        .map_err(|_| RegtestNodeError::InvalidEndpoint)?;
    if uri.scheme_str() != Some("http")
        || !matches!(uri.path(), "" | "/")
        || uri.query().is_some()
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
    {
        return Err(RegtestNodeError::InvalidEndpoint);
    }
    let host = uri
        .host()
        .ok_or(RegtestNodeError::InvalidEndpoint)?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let ip: IpAddr = host
        .parse()
        .map_err(|_| RegtestNodeError::EndpointNotLoopback)?;
    if !ip.is_loopback() {
        return Err(RegtestNodeError::EndpointNotLoopback);
    }
    Ok(endpoint.trim_end_matches('/').to_owned())
}

/// Validates zcashd `getblockchaininfo` data. `chain=regtest` is required, and
/// every network-upgrade activation height supported by the pinned protocol must match.
pub fn verify_node_identity(
    info: &serde_json::Value,
    params: &LocalNetwork,
) -> Result<u64, RegtestNodeError> {
    if params.network_type() != NetworkType::Regtest {
        return Err(RegtestNodeError::WrongNetwork);
    }
    if info.get("chain").and_then(|v| v.as_str()) != Some("regtest") {
        return Err(RegtestNodeError::WrongNetwork);
    }
    let height = info
        .get("blocks")
        .and_then(|v| v.as_u64())
        .filter(|h| *h <= u32::MAX as u64)
        .ok_or(RegtestNodeError::MalformedResponse)?;
    let upgrades = info
        .get("upgrades")
        .and_then(|v| v.as_object())
        .ok_or(RegtestNodeError::MalformedResponse)?;
    for upgrade in [
        NetworkUpgrade::Overwinter,
        NetworkUpgrade::Sapling,
        NetworkUpgrade::Blossom,
        NetworkUpgrade::Heartwood,
        NetworkUpgrade::Canopy,
        NetworkUpgrade::Nu5,
        NetworkUpgrade::Nu6,
        NetworkUpgrade::Nu6_1,
        NetworkUpgrade::Nu6_2,
        NetworkUpgrade::Nu6_3,
    ] {
        let branch = format!("{:08x}", u32::from(upgrade.branch_id()));
        let actual = match upgrades
            .get(&branch)
            .and_then(|v| v.get("activationheight"))
        {
            Some(v) if v.is_null() => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|h| *h <= u32::MAX as u64)
                    .ok_or(RegtestNodeError::MalformedResponse)? as u32,
            ),
            None if params.activation_height(upgrade).is_none() => None,
            None => return Err(RegtestNodeError::MalformedResponse),
        };
        let expected = params.activation_height(upgrade).map(u32::from);
        if actual != expected {
            return Err(RegtestNodeError::ConsensusMismatch);
        }
    }
    Ok(height)
}

/// Query the configured local full node with private RPC credentials and attest its chain.
pub async fn verify_regtest_node(
    config: &FullNodeRpcConfig,
    params: &LocalNetwork,
) -> Result<u64, RegtestNodeError> {
    let info = rpc_call(config, "getblockchaininfo", serde_json::json!([])).await?;
    verify_node_identity(&info, params)
}

/// Confirms lightwalletd's reported tip hash against the independently checked full node.
pub async fn verify_lightwalletd_tip(
    config: &FullNodeRpcConfig,
    params: &LocalNetwork,
    height: u64,
    lightd_hash: &[u8],
) -> Result<(), RegtestNodeError> {
    let node_height = verify_regtest_node(config, params).await?;
    if height == 0 || height > node_height || lightd_hash.len() != 32 {
        return Err(RegtestNodeError::ConsensusMismatch);
    }
    let hash_json = rpc_call(config, "getblockhash", serde_json::json!([height])).await?;
    let hash = hash_json
        .as_str()
        .ok_or(RegtestNodeError::MalformedResponse)?;
    if hash.len() != 64 {
        return Err(RegtestNodeError::MalformedResponse);
    }
    let mut parsed = [0u8; 32];
    for (i, chunk) in hash.as_bytes().chunks_exact(2).enumerate() {
        parsed[i] = u8::from_str_radix(
            std::str::from_utf8(chunk).map_err(|_| RegtestNodeError::MalformedResponse)?,
            16,
        )
        .map_err(|_| RegtestNodeError::MalformedResponse)?;
    }
    // JSON-RPC displays hashes in reverse byte order; compact-block protobuf uses internal order.
    parsed.reverse();
    if parsed.as_slice() != lightd_hash {
        return Err(RegtestNodeError::ConsensusMismatch);
    }
    Ok(())
}

pub async fn rpc_call(
    config: &FullNodeRpcConfig,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, RegtestNodeError> {
    let uri: Uri = config
        .endpoint
        .parse()
        .map_err(|_| RegtestNodeError::InvalidEndpoint)?;
    let host = uri.host().ok_or(RegtestNodeError::InvalidEndpoint)?;
    let port = uri.port_u16().unwrap_or(80);
    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let body =
        serde_json::json!({"jsonrpc":"1.0","id":"qpc-regtest","method":method,"params":params})
            .to_string();
    let credentials = Zeroizing::new(format!("{}:{}", config.username, config.password));
    let auth = Zeroizing::new(STANDARD.encode(credentials.as_bytes()));
    let response = tokio::time::timeout(RPC_TIMEOUT, async {
        let mut stream = TcpStream::connect(address).await.map_err(|_| RegtestNodeError::Connect)?;
        let request = Zeroizing::new(format!("POST / HTTP/1.1\r\nHost: {host}:{port}\r\nAuthorization: Basic {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", auth.as_str(), body.len()));
        stream.write_all(request.as_bytes()).await.map_err(|_| RegtestNodeError::Connect)?;
        let mut bytes = Vec::new();
        stream.take((MAX_RPC_RESPONSE + 1) as u64).read_to_end(&mut bytes).await.map_err(|_| RegtestNodeError::Connect)?;
        if bytes.len() > MAX_RPC_RESPONSE { return Err(RegtestNodeError::MalformedResponse); }
        Ok::<_, RegtestNodeError>(bytes)
    }).await.map_err(|_| RegtestNodeError::Timeout)??;
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(RegtestNodeError::MalformedResponse)?;
    let headers =
        std::str::from_utf8(&response[..split]).map_err(|_| RegtestNodeError::MalformedResponse)?;
    if !headers
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 "))
    {
        return Err(RegtestNodeError::RpcFailure);
    }
    let json: serde_json::Value = serde_json::from_slice(&response[split + 4..])
        .map_err(|_| RegtestNodeError::MalformedResponse)?;
    if !json.get("error").is_some_and(serde_json::Value::is_null) {
        return Err(RegtestNodeError::RpcFailure);
    }
    json.get("result")
        .cloned()
        .ok_or(RegtestNodeError::MalformedResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::consensus::BlockHeight;
    fn params() -> LocalNetwork {
        LocalNetwork {
            overwinter: Some(BlockHeight::from_u32(1)),
            sapling: Some(BlockHeight::from_u32(2)),
            blossom: Some(BlockHeight::from_u32(1)),
            heartwood: Some(BlockHeight::from_u32(1)),
            canopy: Some(BlockHeight::from_u32(1)),
            nu5: Some(BlockHeight::from_u32(1)),
            nu6: Some(BlockHeight::from_u32(1)),
            nu6_1: Some(BlockHeight::from_u32(1)),
            nu6_2: Some(BlockHeight::from_u32(1)),
            nu6_3: Some(BlockHeight::from_u32(1)),
        }
    }
    fn info(chain: &str) -> serde_json::Value {
        let p = params();
        let mut u = serde_json::Map::new();
        for n in [
            NetworkUpgrade::Overwinter,
            NetworkUpgrade::Sapling,
            NetworkUpgrade::Blossom,
            NetworkUpgrade::Heartwood,
            NetworkUpgrade::Canopy,
            NetworkUpgrade::Nu5,
            NetworkUpgrade::Nu6,
            NetworkUpgrade::Nu6_1,
            NetworkUpgrade::Nu6_2,
            NetworkUpgrade::Nu6_3,
        ] {
            let id = format!("{:08x}", u32::from(n.branch_id()));
            let h = p.activation_height(n).map(u32::from);
            u.insert(id, serde_json::json!({"activationheight":h}));
        }
        serde_json::json!({"chain":chain,"blocks":10,"bestblockhash":"11","upgrades":u})
    }
    #[test]
    fn accepts_only_regtest_with_exact_activation_schedule() {
        assert_eq!(verify_node_identity(&info("regtest"), &params()), Ok(10));
    }
    #[test]
    fn rejects_testnet_even_when_loopback_and_consensus_looks_compatible() {
        assert_eq!(
            verify_node_identity(&info("test"), &params()),
            Err(RegtestNodeError::WrongNetwork)
        );
    }
    #[test]
    fn rejects_activation_mismatch_and_malformed_response() {
        let mut x = info("regtest");
        let id = format!("{:08x}", u32::from(NetworkUpgrade::Sapling.branch_id()));
        x["upgrades"][id]["activationheight"] = serde_json::json!(419200);
        assert_eq!(
            verify_node_identity(&x, &params()),
            Err(RegtestNodeError::ConsensusMismatch)
        );
        assert_eq!(
            verify_node_identity(&serde_json::json!({"chain":"regtest"}), &params()),
            Err(RegtestNodeError::MalformedResponse)
        );
    }
    #[test]
    fn endpoint_is_literal_loopback_and_config_debug_hides_password() {
        assert!(validate_rpc_endpoint("http://127.0.0.1:18232").is_ok());
        assert!(validate_rpc_endpoint("http://node:18232").is_err());
        let c = FullNodeRpcConfig::new("http://127.0.0.1:18232", "user", "never-log-this").unwrap();
        assert!(!format!("{c:?}").contains("never-log-this"));
    }
}
