use super::*;
use crate::block::*;
use crate::blockchain::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use bincode::{deserialize, serialize};

use sled;
use tx::TXOutputs;
use log::info;

/*
    An unspent transaction output (UTXO) 

    This is a separate struct to keep track of UTXOs
    utxo is necessary to efficiently find unspent transaction outputs

*/

pub struct UTXOSet{
    pub blockchain: Arc<RwLock<Blockchain>>, // Shared blockchain instance
}

impl UTXOSet {

    pub fn new(blockchain: Arc<RwLock<Blockchain>>) -> Self {
        Self { blockchain }
    }

    // Updates UTXOs
    pub async fn reindex(&self) -> Result<()> {
        if let Err(_e) = std::fs::remove_dir_all("data/utxos") {
            info!("not exist any utxos to delete");
        }
        let db = sled::open("data/utxos")?;

        let blockchain = self.blockchain.read().await;
        let utxos = blockchain.find_utxo();

        for (txid, outs) in utxos {
            db.insert(txid.as_bytes(), serialize(&outs)?)?;
        }

        Ok(())
    }
    
    // Update updates the UTXO set with transactions from the Block
    // The Block is considered to be the tip of a blockchain
    pub fn update(&self, block: &Block) -> Result<()> {
        let db = sled::open("data/utxos")?;

        for tx in block.get_transactions() {
            if !tx.is_coinbase() {
                for vin in &tx.vin {
                    let mut update_outputs = TXOutputs {
                        outputs: Vec::new(),
                    };
                    let outs: TXOutputs = deserialize(&db.get(&vin.txid)?.unwrap().to_vec())?;
                    for out_idx in 0..outs.outputs.len() {
                        if out_idx != vin.vout as usize {
                            update_outputs.outputs.push(outs.outputs[out_idx].clone());
                        }
                    }

                    if update_outputs.outputs.is_empty() {
                        db.remove(&vin.txid)?;
                    } else {
                        db.insert(vin.txid.as_bytes(), serialize(&update_outputs)?)?;
                    }
                }
            }

            let mut new_outputs = TXOutputs {
                outputs: Vec::new(),
            };
            
            for out in &tx.vout {
                new_outputs.outputs.push(out.clone());
            }

            db.insert(tx.id.as_bytes(), serialize(&new_outputs)?)?;
        }
        Ok(())
    }

    /*pub fn get_balance(&self, address: &String) -> Result<i32> {
        let pub_key_hash = Address::decode(address).unwrap().body;

        let utxos: TXOutputs = self.find_utxo(&pub_key_hash)?;

        let mut balance: i32 = 0;
        for out in utxos.outputs {
            balance += out.value;
        }
        println!("Balance of '{}'; {}", address, balance); 

        Ok(balance)
        
    }*/

    pub fn count_transactions(&self) -> Result<i32> {
        let mut counter = 0;
        let db = sled::open("data/utxos")?;
        for kv in db.iter() {
            kv?;
            counter += 1;
        }

        Ok(counter)
    }

    pub fn find_spendable_outputs(&self, pub_key_hash: &[u8], amount: i32) -> Result<(i32, HashMap<String, Vec<i32>>)> {
        let mut unspent_outputs: HashMap<String, Vec<i32>> = HashMap::new();
        let mut accumulated = 0;
        
        let db = sled::open("data/utxos")?;

        for kv in db.iter() {
            let (k, v) = kv?;
            let txid = String::from_utf8(k.to_vec())?;
            let outs: TXOutputs = bincode::deserialize(&v.to_vec())?;
            // txid is the key, outputs are the value

            for out_idx in 0..outs.outputs.len() {
                // Can the output be unlocked with the public key?
                if outs.outputs[out_idx].can_be_unlock_with(pub_key_hash) && accumulated < amount {
                    accumulated += outs.outputs[out_idx].value;
                    match unspent_outputs.get_mut(&txid) {
                        Some(v) => v.push(out_idx as i32),
                        None => {
                            unspent_outputs.insert(txid.clone(), vec![out_idx as i32]);
                        }
                    }
                }
            }
        }

        Ok((accumulated, unspent_outputs))
    }

    /// FindUTXO finds UTXOs for a public key hash
    pub fn find_utxo(&self, pub_key_hash: &[u8]) -> Result<TXOutputs> {
        let mut utxos = TXOutputs {
            outputs: Vec::new(),
        };
        let db = sled::open("data/utxos")?;

        for kv in db.iter() {
            let (_, v) = kv?;
            let outs: TXOutputs = bincode::deserialize(&v.to_vec())?;

            // Goes through all utxos and checks if they are unlocked by that address
            for out in outs.outputs {
                if out.can_be_unlock_with(pub_key_hash) {
                    utxos.outputs.push(out.clone())
                }
            }
        }

        Ok(utxos)
    }

}