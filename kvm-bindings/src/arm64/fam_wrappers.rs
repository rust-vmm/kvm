// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use vmm_sys_util::fam::{FamStruct, FamStructWrapper};
use vmm_sys_util::generate_fam_struct_impl;

use super::bindings::*;

// There is no constant in the kernel as far as the maximum number
// of registers on arm, but KVM_GET_REG_LIST usually returns around 450.
const ARM64_REGS_MAX: usize = 500;

// Implement the FamStruct trait for kvm_reg_list.
generate_fam_struct_impl!(kvm_reg_list, u64, reg, u64, n, ARM64_REGS_MAX);

// Implement the PartialEq trait for kvm_reg_list.
impl PartialEq for kvm_reg_list {
    fn eq(&self, other: &kvm_reg_list) -> bool {
        // No need to call entries's eq, FamStructWrapper's PartialEq will do it for you
        self.n == other.n
    }
}

/// Wrapper over the `kvm_reg_list` structure.
///
/// The `kvm_reg_list` structure contains a flexible array member. For details check the
/// [KVM API](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt)
/// documentation on `kvm_reg_list`. To provide safe access to
/// the array elements, this type is implemented using
/// [FamStructWrapper](../vmm_sys_util/fam/struct.FamStructWrapper.html).
pub type RegList = FamStructWrapper<kvm_reg_list>;

// Implement the FamStruct trait for kvm_irq_routing
generate_fam_struct_impl!(
    kvm_irq_routing,
    kvm_irq_routing_entry,
    entries,
    u32,
    nr,
    1024
);

// Implement the PartialEq trait for kvm_irq_routing.
impl PartialEq for kvm_irq_routing {
    fn eq(&self, other: &kvm_irq_routing) -> bool {
        // No need to call entries's eq, FamStructWrapper's PartialEq will do it for you
        self.nr == other.nr && self.flags == other.flags
    }
}

/// Wrapper over the `kvm_irq_routing` structure.
///
/// The `kvm_irq_routing` structure contains a flexible array member. For details check the [KVM
/// API](https://docs.kernel.org/virt/kvm/api.html#kvm-set-gsi-routing) documentation on
/// `kvm_irq_routing`. To provide safe access to the array elements, this type is implemented using
/// [FamStructWrapper](../vmm_sys_util/fam/struct.FamStructWrapper.html).
pub type KvmIrqRouting = FamStructWrapper<kvm_irq_routing>;

#[cfg(test)]
mod tests {
    use super::KvmIrqRouting;
    use super::RegList;
    use vmm_sys_util::fam::FamStruct;

    #[test]
    fn test_reg_list_eq() {
        let mut wrapper = RegList::new(1).unwrap();
        assert_eq!(wrapper.as_slice().len(), 1);

        let mut wrapper2 = wrapper.clone();
        assert!(wrapper == wrapper2);

        wrapper.as_mut_slice()[0] = 1;
        assert!(wrapper != wrapper2);
        wrapper2.as_mut_slice()[0] = 1;
        assert!(wrapper == wrapper2);
    }
    #[test]
    fn test_kvm_irq_routing() {
        let wrapper = KvmIrqRouting::new(1).unwrap();
        assert_eq!(wrapper.as_slice().len(), 1);
        assert_eq!(wrapper.as_fam_struct_ref().len(), 1);
        assert_eq!(wrapper.as_fam_struct_ref().nr, 1);
    }
}
