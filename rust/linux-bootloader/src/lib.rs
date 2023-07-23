#![no_std]

extern crate alloc;

pub mod efivars;
pub mod linux_loader;
pub mod pe_loader;
pub mod pe_section;
pub mod unified_sections;
pub mod uefi_helpers;
pub mod measure;
pub mod tpm;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
