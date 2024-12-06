use crypto::{digest::Digest, ripemd160::Ripemd160, sha2::Sha256};
use bitcoincash_addr::{Address, HashType, Scheme, Network};
use log::debug;
use serde::{Deserialize, Serialize};
use crate::errors::Result;
//use crate::transaction::hash_pub_key;


#[derive( Serialize, Deserialize, Debug, Clone )]
pub struct TXOutputs {
    pub outputs: Vec<TXOutput>,
}

#[derive( Serialize, Deserialize, Debug, Clone )]
pub struct TXInput {
    pub txid: String,
    pub vout: i32,
    pub signature: Vec<u8>,
    pub pub_key: Vec<u8>,
}

#[derive( Serialize, Deserialize, Debug, Clone )]
pub struct TXOutput {
    pub value: i32,
    pub pub_key_hash: Vec<u8>,
}


impl TXInput {

    // hashes the public_key and returns the address
    pub fn get_address(&self) -> String {
        // Hash the public key first with SHA256
        let mut sha256 = Sha256::new();
        sha256.input(&self.pub_key);
        let sha256_result = sha256.result_str(); // Hex string of SHA256 hash

        // Convert the SHA256 hash back to bytes and apply RIPEMD160
        let mut ripemd160 = Ripemd160::new();
        ripemd160.input(&hex::decode(sha256_result).unwrap());
        let ripemd160_bytes = ripemd160.result_str();

        // Convert the RIPEMD160 result into bytes for the address generation
        let ripemd160_vec = hex::decode(ripemd160_bytes).unwrap();

        let address = Address::new(
            ripemd160_vec,
            Scheme::Base58,       // Choose Base58 or CashAddr
            HashType::Key,        // Public Key Hash
            Network::Main,     // Use Mainnet or Testnet as appropriate
        );
        
        address.encode().unwrap()

    }

    // can_unlock_output_with checks whether the address initiated the transaction
    pub fn can_unlock_output_with(&self, unlocking_data: &[u8]) -> bool {
        let address_string = self.get_address(); // Own the String
        let from_address: &[u8] = address_string.as_bytes();

        from_address == unlocking_data
    } 

}

impl TXOutput {
    // When creating a new output, 
    pub fn new(value: i32, address: String) -> Result<Self> {
        let mut txo = TXOutput {
            value,
            pub_key_hash: Vec::new(),
        };
        txo.lock(&address);
        Ok(txo)
    }

    // "fn checks if the output can be unlocked with the provided data"
    pub fn can_be_unlock_with(&self, unlocking_data: &[u8]) -> bool {
        // you need to ensure that the unlocking_data is consistent in format with the stored pub_key_hash        
        /*println!("can_be_unlock_with() ");
        println!("self.pub_key_hash: {:?} ", &self.pub_key_hash );
        println!("unlocking_data: {:?} \n", &unlocking_data);*/
        
        self.pub_key_hash == unlocking_data
    }

    
    fn lock(&mut self, address: &str) -> Result<()> {
        //println!("lock()");

        let pub_key_hash = Address::decode(address).unwrap().body;
        /*debug!("lock: {}", address);
        println!("pub_key_hash: {:?} \n", pub_key_hash);*/

        self.pub_key_hash = pub_key_hash;

        Ok(())
    } 


}
