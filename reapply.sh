#!/bin/bash
sed -i 's/pub(crate) control: UdpSocket,/pub(crate) control: std::sync::Arc<tokio::net::UdpSocket>,/g' src/connection/manager.rs
sed -i 's/control: ctrl_sock,/control: std::sync::Arc::new(ctrl_sock),/g' src/connection/manager.rs
sed -i 's/control: send_socket,/control: std::sync::Arc::new(send_socket),/g' src/connection/tests.rs
