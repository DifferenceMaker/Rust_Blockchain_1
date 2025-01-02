use std::collections::HashMap;

use failure::format_err;
use log::{debug, info};

use crate::block::Block;
use crate::errors::Result;
use crate::transaction::Transaction;
use crate::tx::TXOutputs;

const TARGET_HEXT: usize = 4;
const GENESIS_COINBASE_DATA: &str =
    "The Times 03/Jan/2009 Chancellor on brink of second bailout for banks";


/*
    Blockhain struct has methods for dealing with UTXOs, Transactions and Blocks.  
*/

#[derive(Debug)]
pub struct Blockchain {
    // tip - top of the blockchain
    pub tip: String,
    pub db: sled::Db,
}

pub struct BlockchainIter<'a> {
    current_hash: String,
    bc: &'a Blockchain,
}

impl Blockchain {

    // Opens an existing blockchain or creates a new one with a fixed coinbase.
    pub fn new() -> Result<Blockchain> {
        let db = sled::open("data/blocks")?;
        let hash = match db.get("LAST")? {
            Some(last_hash) => last_hash.to_vec(),
            None => Vec::new(),
        };

        let lasthash = if hash.is_empty() {
            // If no blocks exist, create the genesis block.
            Blockchain::create_genesis_block(&db)?
        } else {
            String::from_utf8(hash)?
        };

        Ok(Blockchain { tip: lasthash, db })
    }

    /// Creates the genesis block with a fixed coinbase transaction.
    /// Only used when an existing db isn't located on device
    fn create_genesis_block(db: &sled::Db) -> Result<String> {
        let fixed_address = "35yLCpZy2MzPzyngA3YstWbyDhyhzjXBcw".to_string();
        let cbtx = Transaction::new_coinbase(fixed_address, "Genesis Block Reward".to_string())?;
        let genesis = Block::new_genesis_block(cbtx);

        // Insert the genesis block into the database.
        db.insert(genesis.get_hash(), bincode::serialize(&genesis)?)?;
        db.insert("LAST", genesis.get_hash().as_bytes())?;
        db.flush()?;

        Ok( genesis.get_hash() )
    }
    
    // In theory, rarely used
    /*
        - Create a minimal, non-persistent blockchain instance.
        - Use placeholder values for required fields.
        - Avoid side effects like writing to disk or making network calls.
     */
    pub fn default_empty() -> Self {
        Blockchain {
            tip: String::new(), // Empty tip, no blocks
            db: sled::Config::new()
                .temporary(true) // Creates an in-memory database
                .open()
                .expect("Failed to create an in-memory database"),
        }
    }
    /// Creates blockchain with a specific address as the rewardee for genesis block reward
    /// For Custom implementations only
    pub fn create_blockchain(address: String) -> Result<Blockchain> {
        println!("Creating new blockchain");

        std::fs::remove_dir_all("data/blocks").ok();
        let db = sled::open("data/blocks")?;
        debug!("Creating new block database");
        let cbtx = Transaction::new_coinbase(address, String::from(GENESIS_COINBASE_DATA))?;
        let genesis: Block = Block::new_genesis_block(cbtx);
        db.insert(genesis.get_hash(), bincode::serialize(&genesis)?)?;
        db.insert("LAST", genesis.get_hash().as_bytes())?;
        let bc = Blockchain {
            tip: genesis.get_hash(),
            db,
        };
        bc.db.flush()?;
        Ok(bc)
    } 

    // ------------- UTXOs -------------
 
    // Function for finding all the unspent transactions
    fn find_unspent_transactions(&self, address: &[u8]) -> Vec<Transaction> {
        let mut spent_txos: HashMap<String, Vec<i32>> = HashMap::new();
        let mut unspent_txs: Vec<Transaction> = Vec::new();

        for block in self.iter() {
            for tx in block.get_transactions() {
                for index in 0..tx.vout.len() {
                    if let Some(ids) = spent_txos.get(&tx.id){
                        if ids.contains(&(index as i32)) {
                            continue;
                        }
                    }

                    if tx.vout[index].can_be_unlock_with(address) {
                        unspent_txs.push(tx.to_owned());
                    }
                }

                if !tx.is_coinbase() {
                    for i in &tx.vin {
                        if i.can_unlock_output_with(address) {
                            match spent_txos.get_mut(&i.txid) {
                                Some(v) => {
                                    v.push(i.vout);
                                }
                                None => {
                                    spent_txos.insert(i.txid.clone(), vec![i.vout]);
                                }
                            }
                        }
                    }
                }

            }
        }

        unspent_txs

    }

    // Function for finding UTXOs in transactions
    pub fn find_utxo(&self) -> HashMap<String, TXOutputs> {
        let mut utxos: HashMap<String, TXOutputs> = HashMap::new();
        let mut spent_txos: HashMap<String, Vec<i32>> = HashMap::new();


        for block in self.iter() {
            for tx in block.get_transactions() {
                for index in 0..tx.vout.len() {
                    if let Some(ids) = spent_txos.get(&tx.id) {
                        if ids.contains(&(index as i32)) {
                            continue;
                        }
                    }

                    match utxos.get_mut(&tx.id) {
                        Some(v) => {
                            v.outputs.push(tx.vout[index].clone());
                        }
                        None => {
                            utxos.insert(
                                tx.id.clone(),
                                TXOutputs {
                                    outputs: vec![tx.vout[index].clone()],
                                },
                            );
                        }
                    }                    
                }     

                if !tx.is_coinbase() {
                    for i in &tx.vin {
                        match spent_txos.get_mut(&i.txid) {
                            Some(v) => {
                                v.push(i.vout);
                            }
                            None => {
                                spent_txos.insert(i.txid.clone(), vec![i.vout]);
                            }
                        }
                    }
                }
            }
        }
        
        utxos
    }


    pub fn iter(&self) -> BlockchainIter {
        BlockchainIter {
            current_hash: self.tip.clone(),
            bc: &self,
        }
    }

    // ------------- TRANSACTIONS -------------

    // finds a transaction by its ID
    pub fn find_transaction(&self, id: &str) -> Result<Transaction> {
        for b in self.iter() {
            for tx in b.get_transactions() {
                if tx.id == id {
                    return Ok(tx.clone());
                }
            }
        }
        Err(format_err!("Transaction is not found"))
    }

    fn get_prev_txs(&self, tx: &Transaction) -> Result<HashMap<String, Transaction>> {
        let mut prev_txs = HashMap::new();
        for vin in &tx.vin {
            let prev_tx = self.find_transaction(&vin.txid)?;
            prev_txs.insert(prev_tx.id.clone(), prev_tx);
        }
        Ok(prev_txs)
    }

     /// SignTransaction signs inputs of a Transaction
     pub fn sign_transacton(&self, tx: &mut Transaction, private_key: &[u8]) -> Result<()> {
        let prev_txs = self.get_prev_txs(tx)?;
        tx.sign(private_key, prev_txs)?;
        Ok(())
    }

     /// VerifyTransaction verifies transaction input signatures
     pub fn verify_transacton(&self, tx: &Transaction) -> Result<bool> {
        if tx.is_coinbase() {
            return Ok(true);
        }
        let prev_txs = self.get_prev_txs(tx)?;
        tx.verify(prev_txs)
    }

    // ------------- BLOCKS -------------

     /// MineBlock mines a new block with the provided transactions
     pub fn mine_block(&mut self, transactions: Vec<Transaction>) -> Result<Block> {
        /*
            IMPLEMENT NEW COINBASE TRANSACTION AS A REWARD TO THE MINER

            ?? - let cbtx = Transaction::new_coinbase(address, String::from(GENESIS_COINBASE_DATA))?;

         */
        info!("mine a new block");

        // Verifies transactions
        for tx in &transactions {
            if !self.verify_transacton(tx)? {
                return Err(format_err!("ERROR: Invalid transaction"));
            }
        }

        // updates what the last hash is
        let lasthash = self.db.get("LAST")?.unwrap();

        let newblock = Block::new_block(
            transactions,
            String::from_utf8(lasthash.to_vec())?,
            self.get_best_height()? + 1,
        )?;

        // k: hash, v: serialized
        // k: last, v: hash
        self.db.insert(newblock.get_hash(), bincode::serialize(&newblock)?)?;
        self.db.insert("LAST", newblock.get_hash().as_bytes())?;
        self.db.flush()?;

        self.tip = newblock.get_hash();
        Ok(newblock)
    }


    pub fn add_block(&mut self, block: Block) -> Result<()> {
        let data = bincode::serialize(&block)?;
        if let Some(_) = self.db.get(block.get_hash())? {
            return Ok(());
        }
        self.db.insert(block.get_hash(), data)?;

        let lastheight = self.get_best_height()?;
        if block.get_height() > lastheight {
            self.db.insert("LAST", block.get_hash().as_bytes())?;
            self.tip = block.get_hash();
            self.db.flush()?;
        }
        Ok(())
    }

    // GetBlock finds a block by its hash and returns it
    pub fn get_block(&self, block_hash: &str) -> Result<Block> {
        let data = self.db.get(block_hash)?.unwrap();
        let block = bincode::deserialize(&data.to_vec())?;
        Ok(block)
    }

     /// get_best_height returns the height of the latest block
     pub fn get_best_height(&self) -> Result<i32> {
        let lasthash = if let Some(h) = self.db.get("LAST")? {
            h
        } else {
            return Ok(-1);
        };
        let last_data = self.db.get(lasthash)?.unwrap();
        let last_block: Block = bincode::deserialize(&last_data.to_vec())?;
        Ok(last_block.get_height())
    }

    pub fn get_block_hashes(&self) -> Vec<String> {
        let mut list = Vec::new();
        for b in self.iter() {
            list.push(b.get_hash());
        }
        list
    }


}

impl<'a> Iterator for BlockchainIter<'a> {
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        if let Ok(encode_block) = self.bc.db.get(&self.current_hash){
            return match encode_block {
                Some(b) => {
                    if let Ok(block) = bincode::deserialize::<Block>(&b) {
                        self.current_hash = block.get_prev_hash();
                        Some(block)
                    } else {
                        None
                    }
                }
                None => None
            };
        }
        None
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_block() {
        //let mut b = Blockchain::create_blockchain().unwrap();
        let b = Blockchain::new().unwrap();

        /*b.add_block("Data".to_string());
        b.add_block("Data2".to_string());
        b.add_block("data33".to_string());*/
        
        for item in b.iter(){
            println!("item {:?}", item);
        }
    }
}