use crate::errors::Result;
use crate::transaction::Transaction;
use std::time::SystemTime;
use crypto::{ sha2::Sha256, digest::Digest };
use log::info;
use serde::{Deserialize, Serialize};
use merkle_cbt::merkle_tree::Merge;
use merkle_cbt::merkle_tree::CBMT;

const TARGET_HEXT: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    timestamp: u128,
    transactions: Vec<Transaction>,
    prev_block_hash: String,
    hash: String,
    height: i32,
    nonce: i32,
}

impl Block {

    pub fn get_transactions(&self) -> &Vec<Transaction> {
        &self.transactions
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub(crate) fn get_prev_hash(&self) -> String {
        self.prev_block_hash.clone()
    }

    pub fn get_hash(&self) -> String {
        self.hash.clone()
    }

    pub fn new_genesis_block(coinbase: Transaction) -> Block {
        Block::new_block(vec![coinbase], String::new(), 0).unwrap()
    }

    pub fn new_block(
            data: Vec<Transaction>, 
            prev_block_hash: String, 
            height: i32
        ) -> Result<Block> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();

        let mut block = Block {
            timestamp: timestamp,
            transactions: data,
            prev_block_hash,
            hash: String::new(),
            height,
            nonce: 0,
        };
        block.run_proof_of_work()?;
        Ok(block)
    }

    // private function
    fn run_proof_of_work(&mut self) -> Result<()> {
        info!("Mining the block");

        // searches for a nonce for a specific number of 0 requirement.
        while !self.validate()? {
            self.nonce += 1;
        }

        let data = self.prepare_hash_data()?;

        // hashes the block with the new nonce
        let mut hasher = Sha256::new();
        hasher.input(&data[..]);

        // updates block's hash
        self.hash = hasher.result_str();
        Ok(())
    }

    fn hash_transactions(&self) -> Result<Vec<u8>> {
        let mut transactions = Vec::new();
        for tx in &self.transactions {
            transactions.push(tx.hash()?.as_bytes().to_owned());
        }
        let tree = CBMT::<Vec<u8>, MergeTX>::build_merkle_tree(&transactions);

        Ok(tree.root())
    }
    // returns byte array of the hashed block
    fn prepare_hash_data(&self) -> Result<Vec<u8>> {
        let content = (
            self.prev_block_hash.clone(),
            self.hash_transactions()?,
            self.timestamp,
            TARGET_HEXT,
            self.nonce
        );

        let bytes = bincode::serialize(&content)?;
        Ok(bytes)
    }

    // returns 
    fn validate(&self) -> Result<bool> {
        let data = self.prepare_hash_data()?;

        let mut hasher = Sha256::new();
        hasher.input(&data[..]);

        let mut vec1: Vec<u8> = vec![];
        vec1.resize(TARGET_HEXT, '0' as u8);

        Ok(&hasher.result_str()[0..TARGET_HEXT] == String::from_utf8(vec1)?)
    }
}

struct MergeTX {

}

impl Merge for MergeTX {
    type Item = Vec<u8>;
    fn merge(left: &Self::Item, right: &Self::Item) -> Self::Item {
        let mut hasher = Sha256::new();
        let mut data: Vec<u8> = left.clone();
        data.append(&mut right.clone());

        hasher.input(&data);
        let mut re: [u8; 32] = [0; 32];
        hasher.result(&mut re);
        re.to_vec()
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockchain() {
        let mut b = Blockchain::new();
        b.add_block("Data".to_string());
        b.add_block("Data2".to_string());
        b.add_block("data33".to_string());
        dbg!(b);
    }
}*/
