/// Represents a port. This will be used as an resource to indicate which port
/// the server is listening on.
pub struct Port(pub u16);

/// If the executable is started to indicate that TLS should be used for the
/// websocket, this struct will be used to store the certificate related data.
/// It will be used as an optional bevy resource.
pub struct TLSCertificate {
    pub path: String,
    pub password: String,
}
