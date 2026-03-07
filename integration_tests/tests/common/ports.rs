use std::net::TcpListener;

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("Failed to allocate port: {0}")]
    AllocationFailed(std::io::Error),
    #[error("Failed to allocate {count} consecutive ports after {attempts} attempts")]
    ConsecutiveAllocationFailed { count: usize, attempts: usize },
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ReservedPort {
    pub port: u16,
    _listener: Option<TcpListener>,
}

impl ReservedPort {
    #[allow(dead_code)]
    pub fn free(mut self) -> u16 {
        self._listener.take();
        self.port
    }
}

#[allow(dead_code)]
pub fn reserve_port() -> Result<ReservedPort, PortError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(PortError::AllocationFailed)?;
    let port = listener
        .local_addr()
        .map_err(PortError::AllocationFailed)?
        .port();
    Ok(ReservedPort {
        port,
        _listener: Some(listener),
    })
}

#[allow(dead_code)]
pub struct ReservedPorts {
    pub ports: Vec<u16>,
    _listeners: Vec<TcpListener>,
}

impl ReservedPorts {
    #[allow(dead_code)]
    pub fn free(mut self) -> Vec<u16> {
        self._listeners.clear();
        self.ports.clone()
    }
}

#[allow(dead_code)]
pub fn reserve_ports(count: usize) -> Result<ReservedPorts, PortError> {
    let mut listeners = Vec::with_capacity(count);
    let mut ports = Vec::with_capacity(count);

    for _ in 0..count {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(PortError::AllocationFailed)?;
        let port = listener
            .local_addr()
            .map_err(PortError::AllocationFailed)?
            .port();
        listeners.push(listener);
        ports.push(port);
    }

    Ok(ReservedPorts {
        ports,
        _listeners: listeners,
    })
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PortRange {
    pub base: u16,
    pub ports: Vec<u16>,
    _listeners: Vec<TcpListener>,
}

impl PortRange {
    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> u16 {
        self.ports[index]
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.ports.iter().copied()
    }

    #[allow(dead_code)]
    pub fn free(mut self) -> Self {
        self._listeners.clear();
        self
    }
}

#[allow(dead_code)]
pub fn reserve_port_range(count: usize) -> Result<PortRange, PortError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        if attempts > 10 {
            return Err(PortError::ConsecutiveAllocationFailed { count, attempts });
        }

        // Bind to port 0 to get a random base port from the OS
        let base_listener = TcpListener::bind("127.0.0.1:0").map_err(PortError::AllocationFailed)?;
        let base_port = base_listener.local_addr().map_err(PortError::AllocationFailed)?.port();
        drop(base_listener); // Drop it immediately to reuse the base port

        let mut consecutive = true;
        let mut listeners = Vec::with_capacity(count);
        let mut ports = Vec::with_capacity(count);

        for i in 0..count {
            let port = base_port + i as u16;
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) {
                listeners.push(listener);
                ports.push(port);
            } else {
                consecutive = false;
                break;
            }
        }

        if consecutive {
            return Ok(PortRange {
                base: base_port,
                ports,
                _listeners: listeners,
            });
        }
    }
}
