use std::collections::HashMap;
use crate::errors::Result;

use bitcoincash_addr::{Address, HashType, Scheme, Network};
use crypto::{digest::Digest, ripemd160::Ripemd160, sha2::Sha256};
use ed25519_dalek::{SigningKey /* secret key */, Signature, Signer, Verifier, VerifyingKey /* public_key */};

use rand::{RngCore, rngs::OsRng};
use serde::{Serialize, Deserialize};
use log::info;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Wallet {
    pub secret_key: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl Wallet {

    fn new() -> Self {
        
        // Generating keypairs
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng); // secret_key
        let public_key = signing_key.verifying_key(); // public_key

        Wallet {
            secret_key: signing_key.as_bytes().to_vec(),
            public_key: public_key.as_bytes().to_vec(),
        }
    }

    // Reconstruct a wallet from an existing secret key
    pub fn from_secret_key(secret_key: &[u8; 32]) -> Self {
        // Use the secret key to derive the public key
        let signing_key = SigningKey::from_bytes(secret_key);        
        let public_key = signing_key.verifying_key();

        Wallet {
            secret_key: signing_key.as_bytes().to_vec(),
            public_key: public_key.as_bytes().to_vec(),
        }
    }

    // hashes the public_key and returns the address
    pub fn get_address(&self) -> String {
        // Hash the public key first with SHA256
        let mut sha256 = Sha256::new();
        sha256.input(&self.public_key);
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
}

pub struct Wallets {
    // address, Wallet
    wallets: HashMap<String, Wallet>,
}

impl Wallets {

    // returns wallets that are stored on the device's db
    pub fn new() -> Result<Wallets> {
        let mut wlt = Wallets {
            wallets: HashMap::<String, Wallet>::new(),
        };

        let db = sled::open("data/wallets")?;
        for item in db.into_iter() {
            let i = item?;
            let address = String::from_utf8(i.0.to_vec())?;
            let wallet: Wallet = bincode::deserialize(&i.1.to_vec())?;
            
            wlt.wallets.insert(address, wallet);
        }

        drop(db);
        Ok(wlt)
    }
    
    // returns empty Wallets
    pub fn default() -> Wallets {
        Wallets {
            wallets: HashMap::new()
        }
    }

    pub fn get_wallets(&self) -> &HashMap<String, Wallet> {
        &self.wallets
    }

    pub fn get_wallets_mut(&mut self) -> &mut HashMap<String, Wallet> {
        &mut self.wallets
    }

    // Creates a new wallet address but doesn't save it on the db
    pub fn create_wallet(&mut self) -> String {
        let wallet = Wallet::new();
        let address = wallet.get_address();
        self.wallets.insert(address.clone(), wallet);
        info!("Create wallet: {}", address);
        address
    }

    pub fn get_all_address(&self) -> Vec<String> {
        let mut addresses = Vec::new();
        for (address, _) in &self.wallets {
            addresses.push(address.clone());
        }
        addresses
    }

    pub fn get_wallet(&self, address: &str) -> Option<&Wallet> {
        self.wallets.get(address)
    }

    // saves all wallets | Meant as a function at the end of the application runtime
    pub fn save_all(&self) -> Result<()> {
        let db = sled::open("data/wallets")?;

        for (address, wallet) in &self.wallets {
            let data = bincode::serialize(wallet)?;
            db.insert(address, data)?;
        } 

        db.flush()?;
        drop(db);
        Ok(())
    }

    pub fn delete_wallet(&mut self, address: &str) -> Result<()> {
        if self.wallets.remove(address).is_some() {
            let db = sled::open("data/wallets")?;
            db.remove(address)?;  // Remove from the database
            db.flush()?;          // Ensure changes are saved to disk
            Ok(())
        } else {
            Err(failure::err_msg("Wallet not found"))
        }
    }

    pub fn insert(&mut self, address: &str, wlt: Wallet) {
        self.wallets.insert(String::from(address), wlt);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Wallet)> {
        self.wallets.iter()
    }

}
 