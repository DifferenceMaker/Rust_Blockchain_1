// Network

use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{interval, Duration};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use futures::stream::FuturesUnordered;
use failure::format_err;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::transaction::Transaction;
use crate::block::Block;
use crate::utxoset::UTXOSet;

// Shitam jabut public serverim ar blockchain implementation nevis localhost
const KNOWN_NODE1: &str = "127.0.0.1:8335";
const CMD_LEN: usize = 12;
const VERSION: i32 = 1;


#[derive(Serialize, Deserialize, Debug, Clone)]
struct Blockmsg {
    addr_from: String,
    block: Block,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetBlockmsg{
    addr_from: String,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
struct GetDatamsg{
    addr_from: String,
    kind: String,
    id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Invmsg {
    addr_from: String,
    kind: String,
    items: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Txmsg {
    addr_from: String,
    transaction: Transaction,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Versionmsg {
    addr_from: String,
    version: i32,
    best_height: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum Message {
    Addr(Vec<String>),
    Version(Versionmsg),
    Tx(Txmsg),
    GetData(GetDatamsg),
    GetBlock(GetBlockmsg),
    Inv(Invmsg),
    Block(Blockmsg),
}


// - Server -
pub struct Server {
    node_address: String,
    mining_address: String,
    inner: RwLock<ServerInner>,
}

struct ServerInner {
    known_nodes: HashSet<String>,

    // utxo is imported from app.rs, that's why it needs to be Arc. and RwLock.
    utxo: Arc<RwLock<UTXOSet>>,
    blocks_in_transit: Vec<String>,
    mempool: HashMap<String, Transaction>,

}



impl Server {
    pub fn new(port: &str, miner_address: &str, utxo: Arc<RwLock<UTXOSet>>) -> Result<Server> {
        let mut node_set = HashSet::new();
        node_set.insert(String::from(KNOWN_NODE1)); // bootstrap node

        Ok(Server {
            node_address: String::from("0.0.0.0:") + port, // Switch to "localhost:"?
            mining_address: miner_address.to_string(),

            // thread-safe inner
            inner: RwLock::new(ServerInner {
                known_nodes: node_set,
                utxo,
                blocks_in_transit: Vec::new(),
                mempool: HashMap::new(),
            }),
        })
    }

    pub async fn start_server(self: Arc<Self>) -> Result<()> {
        let listener = TcpListener::bind(&self.node_address).await?;
        println!("Start server at {}, mining address: {}", &self.node_address, &self.mining_address);


        // Spawn a task for periodic blockchain state checks
        let server_clone = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval_timer = interval(Duration::from_secs(5));
            loop {
                interval_timer.tick().await;
                if let Err(e) = server_clone.check_and_update_blockchain_state().await {
                    println!("Error during blockchain state check: {}", e);
                }
            }
        });

        // Handle incoming connections
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let server_clone = Arc::clone(&self);
        
                    tokio::spawn(async move {
                        if let Err(e) = server_clone.handle_connection(stream).await {
                            println!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    println!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    // implement shutdown_server

    async fn check_and_update_blockchain_state(&self) -> Result<()> {
        let best_height = self.get_best_height().await?;
        if best_height == -1 {
            self.request_blocks().await?;
        } else {
            self.send_version(KNOWN_NODE1).await?;
        }
        Ok(())
    }

    // Requests blocks from known_nodes
    async fn request_blocks(&self) -> Result<()> {
        for node in self.get_known_nodes().await {
            self.send_get_blocks(&node).await?
        }
        Ok(())
    }

    // ---------------------------------- SENDS ----------------------------------

    async fn send_data(&self, addr: &str, data: &[u8]) -> Result<()> {
        if addr == &self.node_address {
            return Ok(());
        }
        let mut stream = match TcpStream::connect(addr).await {
            Ok(s) => s,
            Err(_) => {
                self.remove_node(addr).await;
                return Ok(());
            }
        };

        let _ = stream.write(data).await;

        println!("data sent successfully");
        Ok(())
    }

    async fn send_block(&self, addr: &str, b: &Block) -> Result<()> {
        println!("send block data to: {} block hash: {}", addr, b.get_hash());
        let data = Blockmsg {
            addr_from: self.node_address.clone(),
            block: b.clone()
        };
        let data = bincode::serialize(&(cmd_to_bytes("block"), data))?;
        self.send_data(addr, &data).await
    }

    async fn send_inv(&self, addr: &str, kind: &str, items: Vec<String>) -> Result<()> {
        println!("send inv message to: {} kind: {} data: {:?}", addr, kind, items);
        let data = Invmsg {
            addr_from: self.node_address.clone(),
            kind: kind.to_string(),
            items,
        };
        let data = bincode::serialize(&(cmd_to_bytes("inv"), data))?;
        self.send_data(addr, &data).await
    }

    pub async fn send_tx(&self, addr: String, tx: &Transaction) -> Result<()> {
        println!("send tx to: {} txid: {}", &addr, &tx.id);
        let data = Txmsg {
            addr_from: self.node_address.clone(),
            transaction: tx.clone(),
        };
        let data = bincode::serialize(&(cmd_to_bytes("tx"), data))?;
        self.send_data(&addr, &data).await
    }

    async fn send_version(&self, addr: &str) -> Result<()> {
        println!("send version info to: {}", addr);
        let data = Versionmsg {
            addr_from: self.node_address.clone(),
            best_height: self.get_best_height().await?,
            version: VERSION,
        };
        let data = bincode::serialize(&(cmd_to_bytes("version"), data))?;
        self.send_data(addr, &data).await
    }

    async fn send_get_blocks(&self, addr: &str) -> Result<()> {
        println!("send get blocks message to: {}", addr);
        let data = GetBlockmsg {
            addr_from: self.node_address.clone(),
        };
        let data = bincode::serialize(&(cmd_to_bytes("getblocks"), data))?;
        self.send_data(addr, &data).await
    }

    async fn send_get_data(&self, addr: &str, kind: &str, id:&str) -> Result<()> {
        println!("send get data message to: {} kind: {} id: {}", addr, kind, id);
        let data = GetDatamsg {
            addr_from: self.node_address.clone(),
            kind: kind.to_string(),
            id: id.to_string(),
        };
        let data = bincode::serialize(&(cmd_to_bytes("getdata"), data))?;
        self.send_data(addr, &data).await

    }

    // sends self.inner.known_nodes to addr
    async fn send_addr(&self, addr: &str) -> Result<()> {
        println!("Send address info to: {}", addr);
        let nodes = self.get_known_nodes().await;
        let data = bincode::serialize(&(cmd_to_bytes("addr"), nodes))?;
        self.send_data(addr, &data).await
    }
    
    // Sends a transaction to every known_node
    pub async fn send_transaction(&self, tx: &Transaction) -> Result<()> {
        println!("Hushhush");
        // There are no nodes. Not even localhost.
        for node in &self.get_known_nodes().await {
            println!("Known_node: {}", node);
        }

        let futures: FuturesUnordered<_> = self.get_known_nodes().await
            .into_iter()
            .map(|node| self.send_tx(node.to_owned(), &tx)) // Pass owned String
            .collect();

        futures.for_each_concurrent(None, |result| async {
            if let Err(e) = result {
                println!("Failed to send transaction: {}", e);
            }
        }).await;

        Ok(())
    }

    // ---------------------------------- HANDLES ----------------------------------

    async fn handle_addr(&self, msg: Vec<String>) -> Result<()> {
        println!("receive address msg: {:#?}", msg);
        for node in msg {
            self.add_nodes(&node).await;
        }
        Ok(())
    }

    // called when a block gets sent to server
    async fn handle_block(&self, msg: Blockmsg) -> Result<()> {
        println!("receive block msg: {}, {}", msg.addr_from, msg.block.get_hash());
        self.add_block(msg.block).await?;

        let mut in_transit = self.get_in_transit().await;
        if in_transit.len() > 0 {
            let block_hash = &in_transit[0];
            self.send_get_data(&msg.addr_from, "block", block_hash).await?;
            in_transit.remove(0);
            self.replace_in_transit(in_transit).await;
        } else {
            self.utxo_reindex().await?;
        }

        Ok(())
    }

    async fn handle_get_blocks(&self, msg: GetBlockmsg) -> Result<()> {
        println!("receive get blocks msg: {:#?}", msg);
        let block_hashes = self.get_block_hashes().await;
        self.send_inv(&msg.addr_from, "block", block_hashes).await?;
        Ok(())
    }

    async fn get_block_hashes(&self) -> Vec<String> {
        self.inner.read().await
            .utxo.read().await
            .blockchain.read().await.get_block_hashes()
    }

    // data = Block or Tx
    async fn handle_get_data(&self, msg: GetDatamsg) -> Result<()> {
        println!("receive get data msg: {:#?}", msg);
        if msg.kind == "block" {
            let block = self.get_block(&msg.id).await?;
            self.send_block(&msg.addr_from, &block).await?;
        } else if msg.kind == "tx" {
            let tx = self.get_mempool_tx(&msg.id).await.unwrap();
            self.send_tx(msg.addr_from, &tx).await?;
        }
        Ok(())
    }

    async fn handle_version(&self, msg: Versionmsg) -> Result<()> {
        println!("receive version msg: {:#?}", msg);

        let my_best_height = self.get_best_height().await?;

        if my_best_height < msg.best_height {
            let _ = self.send_get_blocks(&msg.addr_from).await;
        } else if my_best_height > msg.best_height {
            let _ = self.send_version(&msg.addr_from).await;
        }

        self.send_addr(&msg.addr_from).await?;

        if !self.node_is_known(&msg.addr_from).await {
            self.add_nodes(&msg.addr_from).await;
        }
        Ok(())
    }

    // How to handle a received Tx msg
    async fn handle_tx(&self, msg: Txmsg) -> Result<()> {
        println!("receive tx msg: {} {}", msg.addr_from, &msg.transaction.id);

        self.insert_mempool(msg.transaction.clone()).await;

        let known_nodes = self.get_known_nodes().await;

        if self.node_address == KNOWN_NODE1 {
            for node in known_nodes {
                if node != self.node_address && node != msg.addr_from {
                    self.send_inv(&node, "tx", vec![msg.transaction.id.clone()]).await?;
                }
            }
        } else {
            let mut mempool = self.get_mempool().await;
            println!("Current mempool: {:#?}", &mempool);

            if mempool.len() >= 1 && !self.mining_address.is_empty() {
                loop {
                    let mut txs: Vec<Transaction> = Vec::new();

                    for (_, tx) in &mempool {
                        if self.verify_tx(tx).await? {
                            txs.push(tx.clone());
                        }
                    }

                    if txs.is_empty() {
                        return Ok(());
                    }

                    let cbtx = Transaction::new_coinbase(self.mining_address.clone(), String::new())?;
                    txs.push(cbtx);

                    for tx in &txs {
                        mempool.remove(&tx.id);
                    }

                    let new_block = self.mine_block(txs).await?;
                    self.utxo_reindex().await?;

                    for node in self.get_known_nodes().await {
                        if node != self.node_address {
                            self.send_inv(&node, "block", vec![new_block.get_hash()]).await?;
                        }
                    }

                    if mempool.len() == 0 {
                        break;
                    }
                }
                self.clear_mempool().await;
            }
        }

        Ok(())
    }

    async fn handle_inv(&self, msg: Invmsg) -> Result<()> {
        println!("receive inv msg: {:#?}", msg);

        if msg.kind == "block" {
            let block_hash = &msg.items[0];
            self.send_get_data(&msg.addr_from, "block", block_hash).await?;

            let mut new_in_transit = Vec::new();
            for b in &msg.items {
                if b != block_hash {
                    new_in_transit.push(b.clone());
                }
            }
            self.replace_in_transit(new_in_transit).await;
        } else if msg.kind == "tx" {
            let txid = &msg.items[0];
            match self.get_mempool_tx(txid).await {
                Some(tx) => {
                    if tx.id.is_empty() {
                        self.send_get_data(&msg.addr_from, "tx", txid).await?
                    }
                }
                None => self.send_get_data(&msg.addr_from, "tx", txid).await?
            }
        }

        Ok(())
    }

    // ------------- help functions -------------

    pub async fn get_best_height(&self) -> Result<i32> {
        self.inner.read().await
             .utxo.read().await
             .blockchain.read().await.get_best_height()
    }

    async fn get_mempool_tx(&self, addr: &str) -> Option<Transaction> {
        match self.inner.read().await.mempool.get(addr) {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }

    async fn get_mempool(&self) -> HashMap<String, Transaction> {
        self.inner.read().await.mempool.clone()
    }

    async fn insert_mempool(&self, tx: Transaction) {
        self.inner.write().await.mempool.insert(tx.id.clone(), tx);
    }

    async fn clear_mempool(&self) {
        self.inner.write().await.mempool.clear()
    }

    async fn get_block(&self, block_hash: &str) -> Result<Block> {
        self.inner.read().await
             .utxo.read().await
             .blockchain.read().await.get_block(block_hash)
    }

    async fn verify_tx(&self, tx: &Transaction) -> Result<bool> {
        self.inner.read().await
            .utxo.read().await
            .blockchain.read().await.verify_transacton(tx)
    }

    async fn remove_node(&self, addr: &str) {
        self.inner.write().await.known_nodes.remove(addr);
    }

    async fn add_nodes(&self, addr: &str) {
        self.inner.write().await.known_nodes.insert(String::from(addr));
    }

    async fn get_known_nodes(&self) -> HashSet<String> {
        self.inner.read().await.known_nodes.clone()
    }

    async fn node_is_known(&self, addr: &str) -> bool {
        self.inner.read().await.known_nodes.get(addr).is_some()
    }

    //
    async fn replace_in_transit(&self, hashs: Vec<String>) {
        let bit = &mut self.inner.write().await.blocks_in_transit;
        bit.clone_from(&hashs);
    }

    async fn get_in_transit(&self) -> Vec<String> {
        self.inner.read().await.blocks_in_transit.clone()
    }

    async fn add_block(&self, block: Block) -> Result<()> {
        self.inner.write().await
            .utxo.write().await
            .blockchain.write().await.add_block(block)
    }

    async fn mine_block(&self, txs: Vec<Transaction>) -> Result<Block> {
        self.inner.write().await
            .utxo.write().await
            .blockchain.write().await.mine_block(txs)
    }

    async fn utxo_reindex(&self) -> Result<()> {
        self.inner.write().await
            .utxo.write().await.reindex().await
    }

    // ---------------- Main Handle -------------------

    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let mut buffer = Vec::new();
        let count = stream.read_to_end(&mut buffer).await?;
        println!("Accept request: length {}", count);

        let cmd:Message = bytes_to_cmd(&buffer)?;

        match cmd {
            Message::Addr(data) => self.handle_addr(data).await?,
            Message::Block(data) => self.handle_block(data).await?,
            Message::Inv(data) => self.handle_inv(data).await?,
            Message::GetBlock(data) => self.handle_get_blocks(data).await?,
            Message::GetData(data) => self.handle_get_data(data).await?,
            Message::Tx(data) => self.handle_tx(data).await?,
            Message::Version(data) => self.handle_version(data).await?,
        }

        Ok(())
    }
}

//
fn bytes_to_cmd(bytes: &[u8]) -> Result<Message> {
    let mut cmd = Vec::new();

    // A slice of the first CMD_LEN bytes from bytes
    let cmd_bytes = &bytes[..CMD_LEN];

    //  A slice of the remaining bytes after the command
    let data = &bytes[CMD_LEN..];
    for b in cmd_bytes {
        if 0 as u8 != *b {
            cmd.push(*b);
        }
    }
    println!("cmd: {}", String::from_utf8(cmd.clone())?);

    if cmd == "addr".as_bytes() {
        let data: Vec<String> = bincode::deserialize(data)?;
        Ok(Message::Addr(data))
    } else if cmd == "block".as_bytes() {
        let data: Blockmsg = bincode::deserialize(data)?;
        Ok(Message::Block(data))
    } else if cmd == "inv".as_bytes() {
        let data: Invmsg = bincode::deserialize(data)?;
        Ok(Message::Inv(data))
    } else if cmd == "getblocks".as_bytes() {
        let data: GetBlockmsg = bincode::deserialize(data)?;
        Ok(Message::GetBlock(data))
    } else if cmd == "getdata".as_bytes() {
        let data: GetDatamsg = bincode::deserialize(data)?;
        Ok(Message::GetData(data))
    } else if cmd == "tx".as_bytes() {
        let data: Txmsg = bincode::deserialize(data)?;
        Ok(Message::Tx(data))
    } else if cmd == "version".as_bytes() {
        let data: Versionmsg = bincode::deserialize(data)?;
        Ok(Message::Version(data))
    } else {
        Err(format_err!("Unknown command in the server"))
    }
}

fn cmd_to_bytes(cmd: &str) -> [u8; CMD_LEN] {
    let mut data = [0; CMD_LEN];
    for (i, d) in cmd.as_bytes().iter().enumerate() {
        data[i] = *d;
    }
    data
}