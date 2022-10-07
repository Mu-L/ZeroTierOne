// (c) 2020-2022 ZeroTier, Inc. -- currently propritery pending actual release and licensing. See LICENSE.md.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::vl1::{Address, Identity, InetAddress};
use crate::vl2::certificateofmembership::CertificateOfMembership;
use crate::vl2::certificateofownership::CertificateOfOwnership;
use crate::vl2::rule::Rule;
use crate::vl2::tag::Tag;
use crate::vl2::NetworkId;

use zerotier_utils::buffer::Buffer;
use zerotier_utils::dictionary::Dictionary;
use zerotier_utils::error::InvalidParameterError;
use zerotier_utils::marshalable::Marshalable;

/// Network configuration object sent to nodes by network controllers.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    pub network_id: NetworkId,
    pub issued_to: Address,

    pub name: String,
    pub motd: String,
    pub private: bool,

    pub timestamp: i64,
    pub max_delta: i64,
    pub revision: u64,

    pub mtu: u16,
    pub multicast_limit: u32,
    pub routes: HashSet<IpRoute>,
    pub static_ips: HashSet<InetAddress>,
    pub rules: Vec<Rule>,
    pub dns: HashMap<String, HashSet<InetAddress>>,

    pub certificate_of_membership: Option<CertificateOfMembership>, // considered invalid if None
    pub certificates_of_ownership: Vec<CertificateOfOwnership>,
    pub tags: HashMap<u32, Tag>,

    pub banned: HashSet<Address>,              // v2 only
    pub node_info: HashMap<Address, NodeInfo>, // v2 only

    pub central_url: String,
    pub sso: Option<SSOAuthConfiguration>,
}

impl NetworkConfig {
    pub fn new(network_id: NetworkId, issued_to: Address) -> Self {
        Self {
            network_id,
            issued_to,
            name: String::new(),
            motd: String::new(),
            private: true,
            timestamp: 0,
            max_delta: 0,
            revision: 0,
            mtu: 0,
            multicast_limit: 0,
            routes: HashSet::new(),
            static_ips: HashSet::new(),
            rules: Vec::new(),
            dns: HashMap::new(),
            certificate_of_membership: None,
            certificates_of_ownership: Vec::new(),
            tags: HashMap::new(),
            banned: HashSet::new(),
            node_info: HashMap::new(),
            central_url: String::new(),
            sso: None,
        }
    }

    /// Encode a network configuration for sending to V1 nodes.
    pub fn v1_proto_to_dictionary(&self, controller_identity: &Identity) -> Option<Dictionary> {
        let mut d = Dictionary::new();

        d.set_str(
            proto_v1_field_name::network_config::NETWORK_ID,
            self.network_id.to_string().as_str(),
        );
        if !self.name.is_empty() {
            d.set_str(proto_v1_field_name::network_config::NAME, self.name.as_str());
        }
        d.set_str(proto_v1_field_name::network_config::ISSUED_TO, self.issued_to.to_string().as_str());
        d.set_str(
            proto_v1_field_name::network_config::TYPE,
            if self.private {
                "0"
            } else {
                "1"
            },
        );
        d.set_u64(proto_v1_field_name::network_config::TIMESTAMP, self.timestamp as u64);
        d.set_u64(proto_v1_field_name::network_config::MAX_DELTA, self.max_delta as u64);
        d.set_u64(proto_v1_field_name::network_config::REVISION, self.revision);
        d.set_u64(proto_v1_field_name::network_config::MTU, self.mtu as u64);
        d.set_u64(proto_v1_field_name::network_config::MULTICAST_LIMIT, self.multicast_limit as u64);

        if !self.routes.is_empty() {
            let r: Vec<IpRoute> = self.routes.iter().cloned().collect();
            d.set_bytes(
                proto_v1_field_name::network_config::ROUTES,
                IpRoute::marshal_multiple_to_bytes(r.as_slice()).unwrap(),
            );
        }

        if !self.static_ips.is_empty() {
            let ips: Vec<InetAddress> = self.static_ips.iter().cloned().collect();
            d.set_bytes(
                proto_v1_field_name::network_config::STATIC_IPS,
                InetAddress::marshal_multiple_to_bytes(ips.as_slice()).unwrap(),
            );
        }

        if !self.rules.is_empty() {
            d.set_bytes(
                proto_v1_field_name::network_config::RULES,
                Rule::marshal_multiple_to_bytes(self.rules.as_slice()).unwrap(),
            );
        }

        if !self.dns.is_empty() {
            // NOTE: v1 nodes only support one DNS server per network! If there is more than
            // one the first will be picked, whichever that is (it's a set). The UI should not
            // allow a user to add more than one unless this is a v2-only network.
            let mut dns_bin: Vec<u8> = Vec::with_capacity(256);
            if let Some((name, servers)) = self.dns.iter().next() {
                let mut name_bytes = name.as_bytes();
                name_bytes = &name_bytes[..name_bytes.len().min(127)];
                let _ = dns_bin.write_all(name_bytes);
                for _ in 0..(128 - name_bytes.len()) {
                    dns_bin.push(0);
                }
                for s in servers.iter() {
                    if let Ok(s) = s.to_buffer::<64>() {
                        let _ = dns_bin.write_all(s.as_bytes());
                    }
                }
            }
            d.set_bytes(proto_v1_field_name::network_config::DNS, dns_bin);
        }

        d.set_bytes(
            proto_v1_field_name::network_config::CERTIFICATE_OF_MEMBERSHIP,
            self.certificate_of_membership.as_ref()?.to_bytes()?,
        );

        if !self.certificates_of_ownership.is_empty() {
            let mut certs = Vec::with_capacity(self.certificates_of_ownership.len() * 256);
            for c in self.certificates_of_ownership.iter() {
                let _ = certs.write_all(c.v1_proto_to_bytes(controller_identity.address)?.as_slice());
            }
            d.set_bytes(proto_v1_field_name::network_config::CERTIFICATES_OF_OWNERSHIP, certs);
        }

        if !self.tags.is_empty() {
            let mut certs = Vec::with_capacity(self.tags.len() * 256);
            for (_, t) in self.tags.iter() {
                let _ = certs.write_all(t.v1_proto_to_bytes(controller_identity.address)?.as_slice());
            }
            d.set_bytes(proto_v1_field_name::network_config::TAGS, certs);
        }

        // node_info is not supported by V1 nodes

        if !self.central_url.is_empty() {
            d.set_str(proto_v1_field_name::network_config::CENTRAL_URL, self.central_url.as_str());
        }

        if let Some(sso) = self.sso.as_ref() {
            d.set_bool(proto_v1_field_name::network_config::SSO_ENABLED, true);
            d.set_u64(proto_v1_field_name::network_config::SSO_VERSION, sso.version as u64);
            d.set_str(
                proto_v1_field_name::network_config::SSO_AUTHENTICATION_URL,
                sso.authentication_url.as_str(),
            );
            d.set_u64(
                proto_v1_field_name::network_config::SSO_AUTHENTICATION_EXPIRY_TIME,
                sso.authentication_expiry_time as u64,
            );
            d.set_str(proto_v1_field_name::network_config::SSO_ISSUER_URL, sso.issuer_url.as_str());
            d.set_str(proto_v1_field_name::network_config::SSO_NONCE, sso.nonce.as_str());
            d.set_str(proto_v1_field_name::network_config::SSO_STATE, sso.state.as_str());
            d.set_str(proto_v1_field_name::network_config::SSO_CLIENT_ID, sso.client_id.as_str());
        } else {
            d.set_bool(proto_v1_field_name::network_config::SSO_ENABLED, false);
        }

        Some(d)
    }

    /// Decode a V1 format network configuration.
    pub fn v1_proto_from_dictionary(d: &Dictionary) -> Result<NetworkConfig, InvalidParameterError> {
        let nwid = NetworkId::from_str(
            d.get_str(proto_v1_field_name::network_config::NETWORK_ID)
                .ok_or(InvalidParameterError("missing network ID"))?,
        )
        .map_err(|_| InvalidParameterError("invalid network ID"))?;
        let issued_to_address = Address::from_str(
            d.get_str(proto_v1_field_name::network_config::ISSUED_TO)
                .ok_or(InvalidParameterError("missing address"))?,
        )
        .map_err(|_| InvalidParameterError("invalid address"))?;

        let mut nc = Self::new(nwid, issued_to_address);

        d.get_str(proto_v1_field_name::network_config::NAME)
            .map(|x| nc.name = x.to_string());
        nc.private = d.get_str(proto_v1_field_name::network_config::TYPE).map_or(true, |x| x == "1");
        nc.timestamp = d
            .get_i64(proto_v1_field_name::network_config::TIMESTAMP)
            .ok_or(InvalidParameterError("missing timestamp"))?;
        nc.max_delta = d.get_i64(proto_v1_field_name::network_config::MAX_DELTA).unwrap_or(0);
        nc.revision = d.get_u64(proto_v1_field_name::network_config::REVISION).unwrap_or(0);
        nc.mtu = d
            .get_u64(proto_v1_field_name::network_config::MTU)
            .unwrap_or(crate::protocol::ZEROTIER_VIRTUAL_NETWORK_DEFAULT_MTU as u64) as u16;
        nc.multicast_limit = d.get_u64(proto_v1_field_name::network_config::MULTICAST_LIMIT).unwrap_or(0) as u32;

        if let Some(routes_bin) = d.get_bytes(proto_v1_field_name::network_config::ROUTES) {
            for r in IpRoute::unmarshal_multiple_from_bytes(routes_bin)
                .map_err(|_| InvalidParameterError("invalid route object(s)"))?
                .drain(..)
            {
                let _ = nc.routes.insert(r);
            }
        }

        if let Some(static_ips_bin) = d.get_bytes(proto_v1_field_name::network_config::STATIC_IPS) {
            for ip in InetAddress::unmarshal_multiple_from_bytes(static_ips_bin)
                .map_err(|_| InvalidParameterError("invalid route object(s)"))?
                .drain(..)
            {
                let _ = nc.static_ips.insert(ip);
            }
        }

        if let Some(rules_bin) = d.get_bytes(proto_v1_field_name::network_config::RULES) {
            nc.rules = Rule::unmarshal_multiple_from_bytes(rules_bin).map_err(|_| InvalidParameterError("invalid route object(s)"))?;
        }

        if let Some(dns_bin) = d.get_bytes(proto_v1_field_name::network_config::DNS) {
            if dns_bin.len() > 128 && dns_bin.len() < 1024 {
                let mut name = String::with_capacity(64);
                for i in 0..128 {
                    if dns_bin[i] == 0 {
                        break;
                    } else {
                        name.push(dns_bin[i] as char);
                    }
                }
                if !name.is_empty() {
                    let mut tmp: Buffer<1024> = Buffer::new();
                    let _ = tmp.append_bytes(&dns_bin[128..]);
                    let mut servers = HashSet::new();
                    let mut cursor = 0;
                    while cursor < tmp.len() {
                        if let Ok(s) = InetAddress::unmarshal(&tmp, &mut cursor) {
                            let _ = servers.insert(s);
                        } else {
                            break;
                        }
                    }
                    if !servers.is_empty() {
                        let _ = nc.dns.insert(name, servers);
                    }
                }
            }
        }

        nc.certificate_of_membership = Some(CertificateOfMembership::v1_proto_from_bytes(
            d.get_bytes(proto_v1_field_name::network_config::CERTIFICATE_OF_MEMBERSHIP)
                .ok_or(InvalidParameterError("missing certificate of membership"))?,
        )?);

        if let Some(mut coo_bin) = d.get_bytes(proto_v1_field_name::network_config::CERTIFICATES_OF_OWNERSHIP) {
            while !coo_bin.is_empty() {
                let c = CertificateOfOwnership::v1_proto_from_bytes(coo_bin)?;
                nc.certificates_of_ownership.push(c.0);
                coo_bin = c.1;
            }
        }

        if let Some(mut tag_bin) = d.get_bytes(proto_v1_field_name::network_config::TAGS) {
            while !tag_bin.is_empty() {
                let t = Tag::v1_proto_from_bytes(tag_bin)?;
                let _ = nc.tags.insert(t.0.id, t.0);
                tag_bin = t.1;
            }
        }

        if let Some(central_url) = d.get_str(proto_v1_field_name::network_config::CENTRAL_URL) {
            nc.central_url = central_url.to_string();
        }

        if d.get_bool(proto_v1_field_name::network_config::SSO_ENABLED).unwrap_or(false) {
            nc.sso = Some(SSOAuthConfiguration {
                version: d.get_u64(proto_v1_field_name::network_config::SSO_VERSION).unwrap_or(0) as u32,
                authentication_url: d
                    .get_str(proto_v1_field_name::network_config::SSO_AUTHENTICATION_URL)
                    .unwrap_or("")
                    .to_string(),
                authentication_expiry_time: d
                    .get_i64(proto_v1_field_name::network_config::SSO_AUTHENTICATION_EXPIRY_TIME)
                    .unwrap_or(0),
                issuer_url: d
                    .get_str(proto_v1_field_name::network_config::SSO_ISSUER_URL)
                    .unwrap_or("")
                    .to_string(),
                nonce: d.get_str(proto_v1_field_name::network_config::SSO_NONCE).unwrap_or("").to_string(),
                state: d.get_str(proto_v1_field_name::network_config::SSO_STATE).unwrap_or("").to_string(),
                client_id: d
                    .get_str(proto_v1_field_name::network_config::SSO_CLIENT_ID)
                    .unwrap_or("")
                    .to_string(),
            })
        }

        Ok(nc)
    }
}

#[allow(unused)]
mod proto_v1_field_name {
    pub mod network_config {
        pub const VERSION: &'static str = "v";
        pub const NETWORK_ID: &'static str = "nwid";
        pub const TIMESTAMP: &'static str = "ts";
        pub const REVISION: &'static str = "r";
        pub const ISSUED_TO: &'static str = "id";
        pub const FLAGS: &'static str = "f";
        pub const MULTICAST_LIMIT: &'static str = "ml";
        pub const TYPE: &'static str = "t";
        pub const NAME: &'static str = "n";
        pub const MTU: &'static str = "mtu";
        pub const MAX_DELTA: &'static str = "ctmd";
        pub const CERTIFICATE_OF_MEMBERSHIP: &'static str = "C";
        pub const ROUTES: &'static str = "RT";
        pub const STATIC_IPS: &'static str = "I";
        pub const RULES: &'static str = "R";
        pub const TAGS: &'static str = "TAG";
        pub const CERTIFICATES_OF_OWNERSHIP: &'static str = "COO";
        pub const DNS: &'static str = "DNS";
        pub const NODE_INFO: &'static str = "NI";
        pub const CENTRAL_URL: &'static str = "ssoce";
        pub const SSO_ENABLED: &'static str = "ssoe";
        pub const SSO_VERSION: &'static str = "ssov";
        pub const SSO_AUTHENTICATION_URL: &'static str = "aurl";
        pub const SSO_AUTHENTICATION_EXPIRY_TIME: &'static str = "aexpt";
        pub const SSO_ISSUER_URL: &'static str = "iurl";
        pub const SSO_NONCE: &'static str = "sson";
        pub const SSO_STATE: &'static str = "ssos";
        pub const SSO_CLIENT_ID: &'static str = "ssocid";
    }

    pub mod sso_auth_info {
        pub const VERSION: &'static str = "aV";
        pub const AUTHENTICATION_URL: &'static str = "aU";
        pub const ISSUER_URL: &'static str = "iU";
        pub const CENTRAL_URL: &'static str = "aCU";
        pub const NONCE: &'static str = "aN";
        pub const STATE: &'static str = "aS";
        pub const CLIENT_ID: &'static str = "aCID";
    }
}

/// SSO authentication configuration object.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SSOAuthConfiguration {
    pub version: u32,
    pub authentication_url: String,
    pub authentication_expiry_time: i64,
    pub issuer_url: String,
    pub nonce: String,
    pub state: String,
    pub client_id: String,
}

/// Information about nodes on the network that can be included in a network config.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfo {
    pub flags: u64,
    pub ip: Option<InetAddress>,
    pub name: Option<String>,
    pub services: HashMap<String, Option<String>>,
}

/// Statically pushed L3 IP routes included with a network configuration.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct IpRoute {
    pub target: InetAddress,
    pub via: Option<InetAddress>,
    pub flags: u16,
    pub metric: u16,
}

impl Marshalable for IpRoute {
    const MAX_MARSHAL_SIZE: usize = (InetAddress::MAX_MARSHAL_SIZE * 2) + 2 + 2;

    fn marshal<const BL: usize>(
        &self,
        buf: &mut zerotier_utils::buffer::Buffer<BL>,
    ) -> Result<(), zerotier_utils::marshalable::UnmarshalError> {
        self.target.marshal(buf)?;
        if let Some(via) = self.via.as_ref() {
            via.marshal(buf)?;
        } else {
            buf.append_u8(0)?; // "nil" InetAddress
        }
        buf.append_u16(self.flags)?;
        buf.append_u16(self.metric)?;
        Ok(())
    }

    fn unmarshal<const BL: usize>(
        buf: &zerotier_utils::buffer::Buffer<BL>,
        cursor: &mut usize,
    ) -> Result<Self, zerotier_utils::marshalable::UnmarshalError> {
        Ok(IpRoute {
            target: InetAddress::unmarshal(buf, cursor)?,
            via: {
                let via = InetAddress::unmarshal(buf, cursor)?;
                if via.is_nil() {
                    None
                } else {
                    Some(via)
                }
            },
            flags: buf.read_u16(cursor)?,
            metric: buf.read_u16(cursor)?,
        })
    }
}
