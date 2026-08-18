// Copyright 2024 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::bindings::{
    kvm_irq_routing, kvm_irq_routing_entry, kvm_irq_routing_entry__bindgen_ty_1,
    kvm_irq_routing_msi__bindgen_ty_1, kvm_mp_state, kvm_one_reg, kvm_regs,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zerocopy::{IntoBytes, transmute};

serde_impls! {
    kvm_mp_state,
    kvm_one_reg,
    kvm_regs,
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
        assert_eq!(serialized, serialized_again);
    }

    #[test]
    fn static_assert_serde_implementations() {
        is_serde::<kvm_mp_state>();
        is_serde::<kvm_one_reg>();
        is_serde::<kvm_regs>();
        is_serde::<kvm_irq_routing>();
        is_serde::<kvm_irq_routing_entry>();
    }

    fn is_serde_json<T: Serialize + for<'de> Deserialize<'de> + Default>() {
        let serialized = serde_json::to_string(&T::default()).unwrap();
        let deserialized = serde_json::from_str::<T>(serialized.as_ref()).unwrap();
        let serialized_again = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(serialized, serialized_again);
    }

    #[test]
    fn test_json_serde() {
        is_serde_json::<kvm_mp_state>();
        is_serde_json::<kvm_one_reg>();
        is_serde_json::<kvm_regs>();
        is_serde_json::<kvm_irq_routing>();
        is_serde_json::<kvm_irq_routing_entry>();
    }
}
