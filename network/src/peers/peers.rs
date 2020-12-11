// Copyright (C) 2019-2020 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    external::message_types::{Peers as PeersStruct, *},
    outbound::Request,
    peers::{PeerBook, PeerInfo},
    Environment,
    NetworkError,
    Outbound,
};

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::sync::RwLock;

/// A stateful component for managing the peer connections of this node server.
#[derive(Clone)]
pub struct Peers {
    /// The parameters and settings of this node server.
    environment: Environment,
    /// The outbound service of this node server.
    outbound: Arc<RwLock<Outbound>>,
    /// The list of connected and disconnected peers of this node server.
    peer_book: Arc<RwLock<PeerBook>>,
}

impl Peers {
    ///
    /// Creates a new instance of `Peers`.
    ///
    #[inline]
    pub fn new(environment: Environment, outbound: Arc<RwLock<Outbound>>) -> Result<Self, NetworkError> {
        trace!("Instantiating peer manager");

        // Load the peer book from storage, or create a new peer book.
        let peer_book = PeerBook::default();
        // let peer_book = match PeerBook::load(&*environment.storage_read().await) {
        //     // Case 1 - The peer book was found in storage.
        //     Ok(peer_book) => peer_book,
        //     // Case 2 - Either the peer book does not exist in storage, or could not be deserialized.
        //     // Create a new instance of the peer book.
        //     _ => PeerBook::new(*environment.local_address()),
        // };

        // Instantiate the peer manager.
        let peers = Self {
            environment,
            outbound,
            peer_book: Arc::new(RwLock::new(peer_book)),
        };

        // Save the peer book to storage.
        // peers.save_peer_book_to_storage().await?;

        trace!("Instantiated peer manager");
        Ok(peers)
    }

    ///
    /// Broadcasts updates with connected peers and maintains a permitted number of connected peers.
    ///
    #[inline]
    pub async fn update(&self) -> Result<(), NetworkError> {
        // Fetch the number of connected peers.
        let number_of_connected_peers = self.number_of_connected_peers().await;
        trace!("Connected to {} peers", number_of_connected_peers);

        // Check that this node is not a bootnode.
        if !self.environment.is_bootnode() {
            // Check if this node server is below the permitted number of connected peers.
            if number_of_connected_peers < self.environment.minimum_number_of_connected_peers() {
                // // Broadcast a `GetPeers` message to request for more peers.
                // trace!("Broadcasting getpeers requests to all connected peers");
                // self.broadcast_getpeers_requests().await?;

                // Attempt to connect to the default bootnodes of the network.
                trace!("Broadcasting connection requests to default bootnodes");
                self.connect_to_bootnodes().await?;

                // Attempt to connect to each disconnected peer saved in the peer book.
                trace!("Broadcasting connection requests to disconnected peers");
                self.connect_to_disconnected_peers().await?;
            }
        }

        // Check if this node server is above the permitted number of connected peers.
        if number_of_connected_peers > self.environment.maximum_number_of_connected_peers() {
            // Attempt to connect to the default bootnodes of the network.
            trace!("Disconnect from connected peers to maintain the permitted number");
            // TODO (howardwu): Implement channel closure in the inbound handler,
            //  send channel disconnect messages to those peers from outbound handler,
            //  and close the channels in outbound handler.
            // self.disconnect_from_connected_peers(number_of_connected_peers).await?;

            // v LOGIC TO IMPLEMENT v
            // // Check that the maximum number of peers has not been reached.
            //     warn!("Maximum number of peers is reached, this connection request is being dropped");
            //     match channel.shutdown(Shutdown::Write) {
            // }
        }

        if number_of_connected_peers != 0 {
            // Broadcast a `Version` request to each connected peer.
            self.broadcast_version_requests().await?;

            // Store the peer book to storage.
            self.save_peer_book_to_storage().await?;
        }

        Ok(())
    }

    ///
    /// Returns `true` if the given address is connecting with this node.
    ///
    #[inline]
    pub async fn is_connecting(&self, address: &SocketAddr) -> bool {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch if the given address is connecting in the peer book.
        peer_book.is_connecting(address)
    }

    ///
    /// Returns `true` if the given address is connected with this node.
    ///
    #[inline]
    pub async fn is_connected(&self, address: &SocketAddr) -> bool {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch if the given address is connected in the peer book.
        peer_book.is_connected(address)
    }

    ///
    /// Returns `true` if the given address is a disconnected peer of this node.
    ///
    #[inline]
    pub async fn is_disconnected(&self, address: &SocketAddr) -> bool {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch if the given address is disconnected in the peer book.
        peer_book.is_disconnected(address)
    }

    ///
    /// Returns the number of peers connected to this node.
    ///
    #[inline]
    pub async fn number_of_connected_peers(&self) -> u16 {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch the number of connected peers.
        peer_book.number_of_connected_peers()
    }

    ///
    /// Returns a map of all connected peers with their peer-specific information.
    ///
    #[inline]
    pub async fn connected_peers(&self) -> HashMap<SocketAddr, PeerInfo> {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch the connected peers of this node.
        peer_book.connected_peers().clone()
    }

    ///
    /// Returns a map of all disconnected peers with their peer-specific information.
    ///
    #[inline]
    pub async fn disconnected_peers(&self) -> HashMap<SocketAddr, PeerInfo> {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch the disconnected peers of this node.
        peer_book.disconnected_peers().clone()
    }

    ///
    /// Adds the given address to the disconnected peers in this peer book.
    ///
    #[inline]
    pub async fn found_peer(&self, address: &SocketAddr) -> Result<(), NetworkError> {
        // Acquire the peer book write lock.
        let mut peer_book = self.peer_book.write().await;
        // Add the given address to the peer book.
        peer_book.add_peer(address)
    }

    ///
    /// Returns the local address of the node.
    ///
    #[inline]
    pub fn local_address(&self) -> Option<SocketAddr> {
        // TODO (howardwu): Check that env addr and peer book addr match.
        // // Acquire the peer book reader.
        // let peer_book = self.peer_book.read().await;
        // // Fetch the local address of this node.
        // peer_book.local_address()

        self.environment.local_address()
    }

    ///
    /// Returns the current handshake nonce for the given connected peer.
    ///
    #[inline]
    async fn nonce(&self, remote_address: &SocketAddr) -> Result<u64, NetworkError> {
        // Acquire a peer book read lock.
        let peer_book = self.peer_book.read().await;
        // Fetch the handshake of connected peer.
        peer_book.handshake(remote_address)
    }

    ///
    /// Broadcasts a connection request to all default bootnodes of the network.
    ///
    /// This function attempts to reconnect this node server with any bootnode peer
    /// that this node may have failed to connect to.
    ///
    /// This function filters attempts to connect to itself, and any bootnode peers
    /// this node server is already connected to.
    ///
    #[inline]
    async fn connect_to_bootnodes(&self) -> Result<(), NetworkError> {
        trace!("Connecting to bootnodes");

        // Fetch the local address of this node.
        let local_address = self.local_address().unwrap(); // must be known by now
        // Fetch the current connected peers of this node.
        let connected_peers = self.connected_peers().await;
        // Fetch the current block height of this node.
        let block_height = self.environment.current_block_height().await;

        // Iterate through each bootnode address and attempt a connection request.
        for bootnode_address in self.environment.bootnodes().iter() {
            // Check that this node does not try connecting to itself.
            let is_self = local_address == *bootnode_address;
            // Check that this node does not try reconnecting to a connected peer.
            let is_connected = connected_peers.contains_key(bootnode_address);

            if !is_self && !is_connected {
                // Initialize the `Version` request.
                // TODO (raychu86): Establish a formal node version.
                let version = Version::new_with_rng(1u64, block_height, local_address, *bootnode_address);
                let request = Request::Version(version.clone());

                // Set the bootnode as a connecting peer in the peer book.
                self.connecting_to_peer(bootnode_address, version.nonce).await?;

                // Send a connection request with the outbound handler.
                self.outbound.write().await.broadcast(&request).await;
            }
        }

        Ok(())
    }

    /// Broadcasts a connection request to all disconnected peers.
    #[inline]
    async fn connect_to_disconnected_peers(&self) -> Result<(), NetworkError> {
        // Fetch the local address of this node.
        let local_address = self.local_address().unwrap(); // must be known by now
        // Fetch the current block height of this node.
        let block_height = self.environment.current_block_height().await;

        // Iterate through each connected peer and attempts a connection request.
        for (remote_address, _) in self.disconnected_peers().await {
            // Initialize the `Version` request.
            // TODO (raychu86): Establish a formal node version.
            let version = Version::new_with_rng(1u64, block_height, local_address, remote_address);
            let request = Request::Version(version.clone());

            // Set the disconnected peer as a connecting peer in the peer book.
            self.connecting_to_peer(&remote_address, version.nonce).await?;

            // Send a connection request with the outbound handler.
            self.outbound.write().await.broadcast(&request).await;
        }

        Ok(())
    }

    /// Broadcasts a `Version` message to all connected peers.
    #[inline]
    async fn broadcast_version_requests(&self) -> Result<(), NetworkError> {
        // Get the local address of this node.
        let local_address = self.local_address().unwrap(); // must be known by now
        // Fetch the current block height of this node.
        let block_height = self.environment.current_block_height().await;

        // Broadcast a `Version` message to each connected peer of this node server.
        for (remote_address, _) in self.connected_peers().await {
            debug!("Broadcasting version message to {}", remote_address);

            // Get the handshake nonce.
            if let Ok(nonce) = self.nonce(&remote_address).await {
                // Case 1 - The remote address is of a connected peer and the nonce was retrieved.

                // TODO (raychu86): Establish a formal node version.
                // Broadcast a `Version` message to the connected peer.
                self.outbound
                    .write()
                    .await
                    .broadcast(&Request::Version(Version::new(
                        1u64,
                        block_height,
                        nonce,
                        local_address,
                        remote_address,
                    )))
                    .await;
            } else {
                // Case 2 - The remote address is not of a connected peer, proceed to disconnect.

                // Disconnect from the peer if there is no active connection channel
                // TODO (howardwu): Inform Outbound to also disconnect, by dropping any channels held with this peer.
                self.disconnected_from_peer(&remote_address).await?;
            };
        }

        Ok(())
    }

    /// Broadcasts a `GetPeers` message to all connected peers to request for more peers.
    #[inline]
    async fn broadcast_getpeers_requests(&self) -> Result<(), NetworkError> {
        for (remote_address, _) in self.connected_peers().await {
            // Broadcast a `GetPeers` message to the connected peer.
            self.outbound
                .write()
                .await
                .broadcast(&Request::GetPeers(remote_address, GetPeers))
                .await;

            // // Fetch the connection channel.
            // if let Some(channel) = self.get_channel(&remote_address) {
            //     // Broadcast the message over the channel.
            //     if let Err(_) = channel.write(&GetPeers).await {
            //         // Disconnect from the peer if the message fails to send.
            //         self.disconnected_from_peer(&remote_address).await?;
            //     }
            // } else {
            //     // Disconnect from the peer if the channel is not active.
            //     self.disconnected_from_peer(&remote_address).await?;
            // }
        }

        Ok(())
    }

    /// TODO (howardwu): Implement manual serializers and deserializers to prevent forward breakage
    ///  when the PeerBook or PeerInfo struct fields change.
    ///
    /// Stores the current peer book to the given storage object.
    ///
    /// This function checks that this node is not connected to itself,
    /// and proceeds to serialize the peer book into a byte vector for storage.
    ///
    #[inline]
    async fn save_peer_book_to_storage(&self) -> Result<(), NetworkError> {
        trace!("Saving peer book to storage");

        // Acquire the peer book write lock.
        let mut peer_book = self.peer_book.write().await;
        // Acquire the storage write lock.
        let storage = self.environment.storage_mut().await;

        // Serialize the peer book.
        let serialized_peer_book = bincode::serialize(&*peer_book)?;

        // Save the serialized peer book to storage.
        storage.save_peer_book_to_storage(serialized_peer_book)?;

        trace!("Saved peer book to storage");
        Ok(())
    }
}

impl Peers {
    ///
    /// Sets the given remote address and nonce in the peer book as connecting to this node server.
    ///
    #[inline]
    pub(crate) async fn connecting_to_peer(&self, remote_address: &SocketAddr, nonce: u64) -> Result<(), NetworkError> {
        // Acquire the peer book write lock.
        let mut peer_book = self.peer_book.write().await;
        // Set the peer as connecting with this node server.
        peer_book.set_connecting(remote_address, nonce)
    }

    ///
    /// Sets the given remote address in the peer book as connected to this node server.
    ///
    #[inline]
    pub(crate) async fn connected_to_peer(&self, remote_address: &SocketAddr, nonce: u64) -> Result<(), NetworkError> {
        // Acquire the peer book write lock.
        let mut peer_book = self.peer_book.write().await;
        // Set the peer as connected with this node server.
        peer_book.set_connected(remote_address, nonce)
    }

    /// TODO (howardwu): Add logic to remove the active channels
    ///  and handshakes of the peer from this struct.
    /// Sets the given remote address in the peer book as disconnected from this node server.
    ///
    #[inline]
    pub(crate) async fn disconnected_from_peer(&self, remote_address: &SocketAddr) -> Result<(), NetworkError> {
        // Acquire the peer book write lock.
        let mut peer_book = self.peer_book.write().await;
        // Set the peer as disconnected with this node server.
        peer_book.set_disconnected(remote_address)
        // TODO (howardwu): Attempt to blindly send disconnect message to peer.
    }

    #[inline]
    pub(crate) async fn version_to_verack(
        &self,
        remote_address: SocketAddr,
        remote_version: &Version,
    ) -> Result<(), NetworkError> {
        if self.number_of_connected_peers().await < self.environment.maximum_number_of_connected_peers() {
            debug!("Sending `Verack` request to {}", remote_address);
            self.outbound
                .write()
                .await
                .broadcast(&Request::Verack(Verack::new(
                    remote_version.nonce,
                    remote_version.receiver, /* local_address */
                    remote_address,
                )))
                .await;
            self.connecting_to_peer(&remote_address, remote_version.nonce).await?;
            debug!("Sent `Verack` request to {}", remote_address);
        }

        Ok(())
    }

    #[inline]
    pub(crate) async fn verack(&self, remote_address: &SocketAddr, remote_verack: &Verack) -> Result<(), NetworkError> {
        Ok(())
    }

    #[inline]
    pub(crate) async fn get_peers(&self, remote_address: SocketAddr) -> Result<(), NetworkError> {
        // Add the remote address to the peer book.
        if !self.is_connecting(&remote_address).await && !self.is_connected(&remote_address).await {
            self.found_peer(&remote_address).await?;
        }

        // TODO (howardwu): Simplify this and parallelize this with Rayon.
        // Broadcast the sanitized list of connected peers back to requesting peer.
        let mut peers = Vec::new();
        for (peer_address, peer_info) in self.connected_peers().await {
            // Skip the iteration if the requesting peer that we're sending the response to
            // appears in the list of peers.
            if peer_address == remote_address {
                continue;
            }
            peers.push((peer_address, *peer_info.last_seen()));
        }
        self.outbound
            .write()
            .await
            .broadcast(&Request::Peers(remote_address, PeersStruct::new(peers)))
            .await;

        Ok(())
    }

    /// A miner has sent their list of peer addresses.
    /// Add all new/updated addresses to our disconnected.
    /// The connection handler will be responsible for sending out handshake requests to them.
    #[inline]
    pub(crate) async fn inbound_peers(
        &self,
        remote_address: SocketAddr,
        peers: PeersStruct,
    ) -> Result<(), NetworkError> {
        // Add the remote address to the peer book.
        if !self.is_connecting(&remote_address).await && !self.is_connected(&remote_address).await {
            self.found_peer(&remote_address).await?;
        }

        // TODO (howardwu): Simplify this and parallelize this with Rayon.
        // Process all of the peers sent in the message,
        // by informing the peer book of that we found peers.
        let local_address = self.environment.local_address().unwrap(); // the address must be known by now

        for (peer_address, _) in peers.addresses.iter() {
            // Skip if the peer address is this node's local address.
            let is_zero_address = match "0.0.0.0".to_string().parse::<IpAddr>() {
                Ok(zero_ip) => (*peer_address).ip() == zero_ip,
                _ => false,
            };
            if *peer_address == local_address || is_zero_address {
                continue;
            }
            // Inform the peer book that we found a peer.
            // The peer book will determine if we have seen the peer before,
            // and include the peer if it is new.
            else if !self.is_connecting(peer_address).await && !self.is_connected(peer_address).await {
                self.found_peer(peer_address).await?;
            }
        }

        Ok(())
    }
}
