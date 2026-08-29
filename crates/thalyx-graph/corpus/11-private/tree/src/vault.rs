pub struct Vault;

fn unlock_quietly() -> bool {
    true
}

pub fn open_vault() -> Vault {
    let _ = unlock_quietly();
    Vault
}
