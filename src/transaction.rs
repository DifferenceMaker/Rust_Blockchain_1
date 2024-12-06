use std::collections::HashMap;
use ed25519_dalek::{VerifyingKey, Verifier, SigningKey, Signature, Signer};
use crypto::{digest::Digest, ripemd160::Ripemd160, sha2::Sha256};
use failure::format_err;
use log::{error, info};
use rand::rngs::OsRng;
use rand::RngCore;
use crate::utxoset::UTXOSet;
use crate::wallet::Wallet;
use crate::{ errors::Result, tx::{TXInput, TXOutput}};
use serde::{Deserialize, Serialize};
use bitcoincash_addr::Address;

const SUBSIDY: i32 = 10;


#[derive( Serialize, Deserialize, Debug, Clone )]
pub struct Transaction {
    pub id: String,
    pub vin: Vec<TXInput>,
    pub vout: Vec<TXOutput>,
}

impl Transaction {

    pub fn new_utxo(wallet: &Wallet, to: &str, amount: i32, utxo: &UTXOSet) -> Result<Transaction> {
        println!(
            "new UTXO Transaction from: {} to: {}",
            &wallet.get_address(),
            &to
        );

        let mut vin = Vec::new();
        
        // Raw hash representation for comparison
        let pub_key_hash = Address::decode(&wallet.get_address()).unwrap().body;

        let acc_v = utxo.find_spendable_outputs(&pub_key_hash, amount)?;

        if acc_v.0 < amount {
            error!("Not Enough balance");
            return Err(format_err!(
                "Not Enough balance: current balance {}",
                acc_v.0
            ));
        }

        // Construct transaction inputs (vin)
        for tx in acc_v.1 {
            for out in tx.1 {
                let input = TXInput {
                    txid: tx.0.clone(),
                    vout: out,
                    signature: Vec::new(),
                    pub_key: wallet.public_key.clone(),
                };
                vin.push(input);
            }
        }

        // Construct transaction outputs (vout)
        let mut vout = vec![TXOutput::new(amount, to.to_string())?];

        // If there's change, send it back to the sender's address
        if acc_v.0 > amount {
            vout.push(TXOutput::new(acc_v.0 - amount, wallet.get_address())?);
        }

        // Create the transaction
        let mut tx = Transaction {
            id: String::new(),
            vin,
            vout,
        };

        // Generate the transaction hash
        tx.id = tx.hash()?;

        utxo.blockchain
            .sign_transacton(&mut tx, &wallet.secret_key)?;
        
        Ok(tx)
    }

    pub fn new_coinbase(to: String, mut data: String) -> Result<Transaction> {
        // When does this increase someones coinbase ?
        // Where is this used* ^ 
        println!("new coinbase Transaction to: {}", &to);

        let mut key: [u8; 32] = [0; 32];
        if data.is_empty() {
            let mut rand = OsRng::default();
            rand.fill_bytes(&mut key);
            data = format!("Reward to '{}'", to);
        }

        let mut pub_key = Vec::from(data.as_bytes());
        pub_key.append(&mut Vec::from(key));
        

        // Coinbase Transaction has no id, no txid
        let mut tx = Transaction {
            id: String::new(),
            vin: vec![TXInput {
                txid: String::new(),
                vout: -1,
                signature: Vec::new(),
                pub_key,
            }],
            vout: vec![TXOutput::new(SUBSIDY, to)?],
        };

        tx.id = tx.hash()?;
        Ok(tx)
    }

    pub fn is_coinbase(&self) -> bool {
        self.vin.len() == 1 && self.vin[0].txid.is_empty() && self.vin[0].vout == -1 
    }

    /// Verify verifies signatures of Transaction inputs
    pub fn verify(&self, prev_txs: HashMap<String, Transaction>) -> Result<bool> {
        if self.is_coinbase() {
            return Ok(true);
        }

        for vin in &self.vin {
            if prev_txs.get(&vin.txid).unwrap().id.is_empty() {
                return Err(format_err!("ERROR: Previous transaction is not correct"));
            }
        }

        let mut tx_copy = self.trim_copy();

        for in_id in 0..self.vin.len() {
            let prev_tx = prev_txs.get(&self.vin[in_id].txid).unwrap();

            tx_copy.vin[in_id].signature.clear();
            tx_copy.vin[in_id].pub_key = prev_tx.vout[self.vin[in_id].vout as usize]
                .pub_key_hash
                .clone();
            tx_copy.id = tx_copy.hash()?;
            tx_copy.vin[in_id].pub_key = Vec::new();

        
             // Convert public key and signature from bytes
            let public_key_bytes = &self.vin[in_id].pub_key;
            let signature_bytes = &self.vin[in_id].signature;

            // Ensure the public key and signature lengths are valid
            if public_key_bytes.len() != 32 || signature_bytes.len() != 64 {
                return Err(format_err!("Invalid public key or signature length"));
            }

             // Convert public key and signature from Vec<u8> to fixed-size arrays
            let public_key_array: &[u8; 32] = public_key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| format_err!("Failed to convert public key to fixed-size array"))?;
            let signature_array: &[u8; 64] = signature_bytes
                .as_slice()
                .try_into()
                .map_err(|_| format_err!("Failed to convert signature to fixed-size array"))?;

             // Create the PublicKey and Signature objects
            let public_key = VerifyingKey::from_bytes(public_key_array)
                .map_err(|_| format_err!("Failed to parse public key"))?;
            let signature = Signature::from_bytes(signature_array);

                // Verify the signature
            if public_key.verify(tx_copy.id.as_bytes(), &signature).is_err() {
                return Ok(false); // Verification failed
            }
            
        }

        Ok(true)
    }

    pub fn sign(&mut self, private_key: &[u8], prev_txs: HashMap<String, Transaction>) -> Result<()> {
        if self.is_coinbase() {
            return Ok(())
        }

        // Ensure the private key is the correct length
        if private_key.len() != 32 {
            return Err(format_err!("Invalid private key length"));
        }

         // Convert the private key slice to a fixed-size array
        let private_key_bytes: &[u8; 32] = private_key
            .try_into()
            .map_err(|_| format_err!("Private key must be 32 bytes"))?;

        // Create a SigningKey from the private key bytes
        let signing_key = SigningKey::from_bytes(private_key_bytes);

        for vin in &self.vin {
            if prev_txs.get(&vin.txid).unwrap().id.is_empty() {
                return Err(format_err!("Error: Previous transaction is not corrent"));
            }
        }
        let mut tx_copy = self.trim_copy();

        for in_id in 0..tx_copy.vin.len() {
            let prev_tx = prev_txs.get(&tx_copy.vin[in_id].txid).unwrap();

            // Clear signature and set the public key in the transaction input
            tx_copy.vin[in_id].signature.clear();
            tx_copy.vin[in_id].pub_key = prev_tx.vout[tx_copy.vin[in_id].vout as usize]
                .pub_key_hash
                .clone();
            
            // Hash the transaction copy
            tx_copy.id = tx_copy.hash()?;
            tx_copy.vin[in_id].pub_key = Vec::new(); // Clear public key for signing

            // Sign the transaction hash
            let signature = signing_key.sign(tx_copy.id.as_bytes());

             // Store the signature in the original transaction input
            self.vin[in_id].signature = signature.to_bytes().to_vec();
            //.to_bytes().to_vec();
        }

        Ok(())
    }

    pub fn hash(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.id = String::new();
        let data = bincode::serialize(&copy)?;
        let mut hasher = Sha256::new();
        hasher.input(&data[..]);
        Ok(hasher.result_str())
    }

    fn trim_copy(&self) -> Transaction {
        let mut vin = Vec::new();
        let mut vout = Vec::new();

        for v in &self.vin{
            vin.push(TXInput {
                txid: v.txid.clone(),
                vout: v.vout.clone(),
                signature: Vec::new(),
                pub_key: Vec::new(),
            });
        }

        for v in &self.vout {
            vout.push( TXOutput {
                value: v.value,
                pub_key_hash: v.pub_key_hash.clone(),
            });
        }

        Transaction {
            id: self.id.clone(),
            vin,
            vout,
        }
    }



}

/*pub fn hash_pub_key(pub_key: &mut Vec<u8>) {
    let mut hasher1 = Sha256::new();
    hasher1.input(pub_key);
    hasher1.result(pub_key);
    
    let mut hasher2 = Ripemd160::new();
    hasher2.input(pub_key);
    pub_key.resize(20, 0);
    hasher2.result(pub_key);
}*/