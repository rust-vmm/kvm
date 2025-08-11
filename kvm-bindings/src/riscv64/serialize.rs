// Copyright 2024 © Institute of Software, CAS. All rights reserved.
// Copyright 2024 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::bindings::{
    kvm_irq_routing, kvm_irq_routing_entry, kvm_irq_routing_entry__bindgen_ty_1,
    kvm_irq_routing_msi__bindgen_ty_1, kvm_mp_state, kvm_one_reg, kvm_riscv_aia_csr,
    kvm_riscv_config, kvm_riscv_core, kvm_riscv_csr, kvm_riscv_sbi_sta, kvm_riscv_smstateen_csr,
    kvm_riscv_timer, user_regs_struct,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zerocopy::{IntoBytes, transmute};

serde_impls! {
    kvm_mp_state,
    kvm_one_reg,
    kvm_riscv_config,
    kvm_riscv_core,
    user_regs_struct,
    kvm_riscv_csr,
    kvm_riscv_aia_csr,
    kvm_riscv_smstateen_csr,
    kvm_riscv_timer,
    kvm_riscv_sbi_sta,
    kvm_irq_routing,
    kvm_irq_routing_entry
}

// SAFETY: zerocopy's derives explicitly disallow deriving for unions where
// the fields have different sizes, due to the smaller fields having padding.
// Miri however does not complain about these implementations (e.g. about
// reading the "padding" for one union field as valid data for a bigger one)
unsafe impl IntoBytes for kvm_irq_routing_msi__bindgen_ty_1 {
    fn only_derive_is_allowed_to_implement_this_trait()
    where
        Self: Sized,
    {
    }
}

// SAFETY: zerocopy's derives explicitly disallow deriving for unions where
// the fields have different sizes, due to the smaller fields having padding.
// Miri however does not complain about these implementations (e.g. about
// reading the "padding" for one union field as valid data for a bigger one)
unsafe impl IntoBytes for kvm_irq_routing_entry__bindgen_ty_1 {
    fn only_derive_is_allowed_to_implement_this_trait()
    where
        Self: Sized,
    {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn is_serde<T: Serialize + for<'de> Deserialize<'de> + Default>() {
        let config = bincode::config::standard();
        let serialized = bincode::serde::encode_to_vec(T::default(), config).unwrap();
        let (deserialized, _): (T, _) =
            bincode::serde::decode_from_slice(&serialized, config).unwrap();
        let serialized_again = bincode::serde::encode_to_vec(&deserialized, config).unwrap();
        // Compare the serialized state after a roundtrip, to work around issues with
        // bindings not implementing `PartialEq`.
        assert_eq!(serialized, serialized_again);
    }

    #[test]
    fn static_assert_serde_implementations() {
        // This test statically (= at compile-time) asserts that various bindgen generated
        // structures implement serde's `Serialize` and `Deserialize` traits.
        // This is to make sure that we do not accidentally remove those implementations
        // when regenerating bindings. If this test fails to compile, please add
        //
        // #[cfg_attr(
        //     feature = "serde",
        //     derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
        // )]
        //
        // to all structures causing compilation errors (we need the zerocopy traits, as the
        // `Serialize` and `Deserialize` implementations are provided by the `serde_impls!` macro
        // above, which implements serialization based on zerocopy's `FromBytes` and `IntoBytes`
        // traits that it expects to be derived).
        //
        // NOTE: This only include "top-level" items, and does not list out bindgen-anonymous types
        // (e.g. types like `kvm_vcpu_events__bindgen_ty_5`). These types can change name across
        // bindgen versions. If after re-adding the derives to all the below items you can compile
        // errors about anonymous types not implementing `Serialize`/`Deserialize`, please also add
        // the derives to all anonymous types references in the definitions of the below items.

        is_serde::<kvm_mp_state>();
        is_serde::<kvm_one_reg>();
        is_serde::<kvm_riscv_config>();
        is_serde::<kvm_riscv_core>();
        is_serde::<user_regs_struct>();
        is_serde::<kvm_riscv_csr>();
        is_serde::<kvm_riscv_aia_csr>();
        is_serde::<kvm_riscv_smstateen_csr>();
        is_serde::<kvm_riscv_timer>();
        is_serde::<kvm_riscv_sbi_sta>();
        is_serde::<kvm_irq_routing>();
        is_serde::<kvm_irq_routing_entry>();
    }

    fn is_serde_json<T: Serialize + for<'de> Deserialize<'de> + Default>() {
        let config = bincode::config::standard();
        let serialized = bincode::serde::encode_to_vec(T::default(), config).unwrap();
        let (deserialized, _): (T, _) =
            bincode::serde::decode_from_slice(&serialized, config).unwrap();
        let serialized_again = bincode::serde::encode_to_vec(&deserialized, config).unwrap();
        // Compare the serialized state after a roundtrip, to work around issues with
        // bindings not implementing `PartialEq`.
        assert_eq!(serialized, serialized_again);
    }

    #[test]
    fn test_json_serde() {
        is_serde_json::<kvm_mp_state>();
        is_serde_json::<kvm_one_reg>();
        is_serde_json::<kvm_riscv_config>();
        is_serde_json::<kvm_riscv_core>();
        is_serde_json::<user_regs_struct>();
        is_serde_json::<kvm_riscv_csr>();
        is_serde_json::<kvm_riscv_aia_csr>();
        is_serde_json::<kvm_riscv_smstateen_csr>();
        is_serde_json::<kvm_riscv_timer>();
        is_serde_json::<kvm_riscv_sbi_sta>();
        is_serde_json::<kvm_irq_routing>();
        is_serde_json::<kvm_irq_routing_entry>();
    }
}
