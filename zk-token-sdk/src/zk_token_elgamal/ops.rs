pub use target_arch::*;

#[cfg(not(target_arch = "bpf"))]
mod target_arch {
    use {
        crate::{encryption::elgamal::ElGamalCiphertext, zk_token_elgamal::pod},
        curve25519_dalek::{constants::RISTRETTO_BASEPOINT_COMPRESSED, scalar::Scalar},
        std::convert::TryInto,
    };
    pub const TWO_32: u64 = 4294967296;

    // On input two scalars x0, x1 and two ciphertexts ct0, ct1,
    // returns `Some(x0*ct0 + x1*ct1)` or `None` if the input was invalid
    fn add_ciphertexts(
        scalar_0: Scalar,
        ct_0: &pod::ElGamalCiphertext,
        scalar_1: Scalar,
        ct_1: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let ct_0: ElGamalCiphertext = (*ct_0).try_into().ok()?;
        let ct_1: ElGamalCiphertext = (*ct_1).try_into().ok()?;

        let ct_sum = ct_0 * scalar_0 + ct_1 * scalar_1;
        Some(pod::ElGamalCiphertext::from(ct_sum))
    }

    pub(crate) fn combine_lo_hi(
        ct_lo: &pod::ElGamalCiphertext,
        ct_hi: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        add_ciphertexts(Scalar::one(), ct_lo, Scalar::from(TWO_32), ct_hi)
    }

    pub fn add(
        ct_0: &pod::ElGamalCiphertext,
        ct_1: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        add_ciphertexts(Scalar::one(), ct_0, Scalar::one(), ct_1)
    }

    pub fn add_with_lo_hi(
        ct_0: &pod::ElGamalCiphertext,
        ct_1_lo: &pod::ElGamalCiphertext,
        ct_1_hi: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let ct_1 = combine_lo_hi(ct_1_lo, ct_1_hi)?;
        add_ciphertexts(Scalar::one(), ct_0, Scalar::one(), &ct_1)
    }

    pub fn subtract(
        ct_0: &pod::ElGamalCiphertext,
        ct_1: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        add_ciphertexts(Scalar::one(), ct_0, -Scalar::one(), ct_1)
    }

    pub fn subtract_with_lo_hi(
        ct_0: &pod::ElGamalCiphertext,
        ct_1_lo: &pod::ElGamalCiphertext,
        ct_1_hi: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let ct_1 = combine_lo_hi(ct_1_lo, ct_1_hi)?;
        add_ciphertexts(Scalar::one(), ct_0, -Scalar::one(), &ct_1)
    }

    pub fn add_to(ct: &pod::ElGamalCiphertext, amount: u64) -> Option<pod::ElGamalCiphertext> {
        let mut amount_as_ct = [0_u8; 64];
        amount_as_ct[..32].copy_from_slice(RISTRETTO_BASEPOINT_COMPRESSED.as_bytes());
        add_ciphertexts(
            Scalar::one(),
            ct,
            Scalar::from(amount),
            &pod::ElGamalCiphertext(amount_as_ct),
        )
    }

    pub fn subtract_from(
        ct: &pod::ElGamalCiphertext,
        amount: u64,
    ) -> Option<pod::ElGamalCiphertext> {
        let mut amount_as_ct = [0_u8; 64];
        amount_as_ct[..32].copy_from_slice(RISTRETTO_BASEPOINT_COMPRESSED.as_bytes());
        add_ciphertexts(
            Scalar::one(),
            ct,
            -Scalar::from(amount),
            &pod::ElGamalCiphertext(amount_as_ct),
        )
    }
}

#[cfg(target_arch = "bpf")]
#[allow(unused_variables)]
mod target_arch {
    use crate::{
        curve25519::{
            ristretto::{add_ristretto, multiply_ristretto, subtract_ristretto, PodRistrettoPoint},
            scalar::PodScalar,
        },
        zk_token_elgamal::pod,
    };

    const SHIFT_BITS: usize = 16;

    const G: PodRistrettoPoint = PodRistrettoPoint([
        226, 242, 174, 10, 106, 188, 78, 113, 168, 132, 169, 97, 197, 0, 81, 95, 88, 227, 11, 106,
        165, 130, 221, 141, 182, 166, 89, 69, 224, 141, 45, 118,
    ]);

    pub fn add(
        left_ciphertext: &pod::ElGamalCiphertext,
        right_ciphertext: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let (left_commitment, left_handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*left_ciphertext).into();
        let (right_commitment, right_handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*right_ciphertext).into();

        let result_commitment: pod::PedersenCommitment =
            add_ristretto(&left_commitment.into(), &right_commitment.into())?.into();
        let result_handle: pod::DecryptHandle =
            add_ristretto(&left_handle.into(), &right_handle.into())?.into();

        Some((result_commitment, result_handle).into())
    }

    pub fn add_with_lo_hi(
        left_ciphertext: &pod::ElGamalCiphertext,
        right_ciphertext_lo: &pod::ElGamalCiphertext,
        right_ciphertext_hi: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let shift_scalar = to_scalar(1_u64 << SHIFT_BITS);
        let shifted_right_ciphertext_hi = scalar_ciphertext(&shift_scalar, &right_ciphertext_hi)?;
        let combined_right_ciphertext = add(right_ciphertext_lo, &shifted_right_ciphertext_hi)?;
        add(left_ciphertext, &combined_right_ciphertext)
    }

    pub fn subtract(
        left_ciphertext: &pod::ElGamalCiphertext,
        right_ciphertext: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let (left_commitment, left_handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*left_ciphertext).into();
        let (right_commitment, right_handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*right_ciphertext).into();

        let result_commitment: pod::PedersenCommitment =
            subtract_ristretto(&left_commitment.into(), &right_commitment.into())?.into();
        let result_handle: pod::DecryptHandle =
            subtract_ristretto(&left_handle.into(), &right_handle.into())?.into();

        Some((result_commitment, result_handle).into())
    }

    pub fn subtract_with_lo_hi(
        left_ciphertext: &pod::ElGamalCiphertext,
        right_ciphertext_lo: &pod::ElGamalCiphertext,
        right_ciphertext_hi: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let shift_scalar = to_scalar(1_u64 << SHIFT_BITS);
        let shifted_right_ciphertext_hi = scalar_ciphertext(&shift_scalar, &right_ciphertext_hi)?;
        let combined_right_ciphertext = add(right_ciphertext_lo, &shifted_right_ciphertext_hi)?;
        subtract(left_ciphertext, &combined_right_ciphertext)
    }

    pub fn add_to(
        ciphertext: &pod::ElGamalCiphertext,
        amount: u64,
    ) -> Option<pod::ElGamalCiphertext> {
        let amount_scalar = to_scalar(amount);
        let amount_point = multiply_ristretto(&amount_scalar, &G)?;

        let (commitment, handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*ciphertext).into();
        let commitment_point: PodRistrettoPoint = commitment.into();

        let result_commitment: pod::PedersenCommitment =
            add_ristretto(&commitment_point, &amount_point)?.into();
        Some((result_commitment, handle).into())
    }

    pub fn subtract_from(
        ciphertext: &pod::ElGamalCiphertext,
        amount: u64,
    ) -> Option<pod::ElGamalCiphertext> {
        let amount_scalar = to_scalar(amount);
        let amount_point = multiply_ristretto(&amount_scalar, &G)?;

        let (commitment, handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*ciphertext).into();
        let commitment_point: PodRistrettoPoint = commitment.into();

        let result_commitment: pod::PedersenCommitment =
            subtract_ristretto(&commitment_point, &amount_point)?.into();
        Some((result_commitment, handle).into())
    }

    fn to_scalar(amount: u64) -> PodScalar {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&amount.to_le_bytes());
        PodScalar(bytes)
    }

    fn scalar_ciphertext(
        scalar: &PodScalar,
        ciphertext: &pod::ElGamalCiphertext,
    ) -> Option<pod::ElGamalCiphertext> {
        let (commitment, handle): (pod::PedersenCommitment, pod::DecryptHandle) =
            (*ciphertext).into();

        let commitment_point: PodRistrettoPoint = commitment.into();
        let handle_point: PodRistrettoPoint = handle.into();

        let result_commitment: pod::PedersenCommitment =
            multiply_ristretto(scalar, &commitment_point)?.into();
        let result_handle: pod::DecryptHandle = multiply_ristretto(scalar, &handle_point)?.into();

        Some((result_commitment, result_handle).into())
    }
}

pub const OP_ADD: u64 = 0;
pub const OP_SUB: u64 = 1;

extern "C" {
    pub fn sol_zk_token_elgamal_op(
        op: u64,
        ct_0: *const u8,
        ct_1: *const u8,
        ct_result: *mut u8,
    ) -> u64;
    pub fn sol_zk_token_elgamal_op_with_lo_hi(
        op: u64,
        ct_0: *const u8,
        ct_1_lo: *const u8,
        ct_1_hi: *const u8,
        ct_result: *mut u8,
    ) -> u64;
    pub fn sol_zk_token_elgamal_op_with_scalar(
        op: u64,
        ct: *const u8,
        scalar: u64,
        ct_result: *mut u8,
    ) -> u64;
}

#[cfg(test)]
mod tests {
    use {
        crate::{
            encryption::{
                elgamal::{ElGamalCiphertext, ElGamalKeypair},
                pedersen::{Pedersen, PedersenOpening},
            },
            zk_token_elgamal::{ops, pod},
        },
        bytemuck::Zeroable,
        curve25519_dalek::scalar::Scalar,
        std::convert::TryInto,
    };

    #[test]
    fn test_zero_ct() {
        let spendable_balance = pod::ElGamalCiphertext::zeroed();
        let spendable_ct: ElGamalCiphertext = spendable_balance.try_into().unwrap();

        // spendable_ct should be an encryption of 0 for any public key when
        // `PedersenOpen::default()` is used
        let public = ElGamalKeypair::new_rand().public;
        let balance: u64 = 0;
        assert_eq!(
            spendable_ct,
            public.encrypt_with(balance, &PedersenOpening::default())
        );

        // homomorphism should work like any other ciphertext
        let open = PedersenOpening::new_rand();
        let transfer_amount_ct = public.encrypt_with(55_u64, &open);
        let transfer_amount_pod: pod::ElGamalCiphertext = transfer_amount_ct.into();

        let sum = ops::add(&spendable_balance, &transfer_amount_pod).unwrap();

        let expected: pod::ElGamalCiphertext = public.encrypt_with(55_u64, &open).into();
        assert_eq!(expected, sum);
    }

    #[test]
    fn test_add_to() {
        let spendable_balance = pod::ElGamalCiphertext::zeroed();

        let added_ct = ops::add_to(&spendable_balance, 55).unwrap();

        let public = ElGamalKeypair::new_rand().public;
        let expected: pod::ElGamalCiphertext = public
            .encrypt_with(55_u64, &PedersenOpening::default())
            .into();

        assert_eq!(expected, added_ct);
    }

    #[test]
    fn test_subtract_from() {
        let amount = 77_u64;
        let public = ElGamalKeypair::new_rand().public;
        let open = PedersenOpening::new_rand();
        let encrypted_amount: pod::ElGamalCiphertext = public.encrypt_with(amount, &open).into();

        let subtracted_ct = ops::subtract_from(&encrypted_amount, 55).unwrap();

        let expected: pod::ElGamalCiphertext = public.encrypt_with(22_u64, &open).into();

        assert_eq!(expected, subtracted_ct);
    }

    /// Split u64 number into two u32 numbers
    fn split_u64_into_u32(amt: u64) -> (u32, u32) {
        let lo = amt as u32;
        let hi = (amt >> 32) as u32;

        (lo, hi)
    }

    #[test]
    fn test_transfer_arithmetic() {
        // transfer amount
        let transfer_amount: u64 = 55;
        let (amount_lo, amount_hi) = split_u64_into_u32(transfer_amount);

        // generate public keys
        let source_pk = ElGamalKeypair::new_rand().public;
        let dest_pk = ElGamalKeypair::new_rand().public;
        let auditor_pk = ElGamalKeypair::new_rand().public;

        // commitments associated with TransferRangeProof
        let (comm_lo, open_lo) = Pedersen::new(amount_lo);
        let (comm_hi, open_hi) = Pedersen::new(amount_hi);

        let comm_lo: pod::PedersenCommitment = comm_lo.into();
        let comm_hi: pod::PedersenCommitment = comm_hi.into();

        // decryption handles associated with TransferValidityProof
        let handle_source_lo: pod::DecryptHandle = source_pk.decrypt_handle(&open_lo).into();
        let handle_dest_lo: pod::DecryptHandle = dest_pk.decrypt_handle(&open_lo).into();
        let _handle_auditor_lo: pod::DecryptHandle = auditor_pk.decrypt_handle(&open_lo).into();

        let handle_source_hi: pod::DecryptHandle = source_pk.decrypt_handle(&open_hi).into();
        let handle_dest_hi: pod::DecryptHandle = dest_pk.decrypt_handle(&open_hi).into();
        let _handle_auditor_hi: pod::DecryptHandle = auditor_pk.decrypt_handle(&open_hi).into();

        // source spendable and recipient pending
        let source_open = PedersenOpening::new_rand();
        let dest_open = PedersenOpening::new_rand();

        let source_spendable_ct: pod::ElGamalCiphertext =
            source_pk.encrypt_with(77_u64, &source_open).into();
        let dest_pending_ct: pod::ElGamalCiphertext =
            dest_pk.encrypt_with(77_u64, &dest_open).into();

        // program arithmetic for the source account

        // 1. Combine commitments and handles
        let source_lo_ct: pod::ElGamalCiphertext = (comm_lo, handle_source_lo).into();
        let source_hi_ct: pod::ElGamalCiphertext = (comm_hi, handle_source_hi).into();

        // 2. Combine lo and hi ciphertexts
        let source_combined_ct = ops::combine_lo_hi(&source_lo_ct, &source_hi_ct).unwrap();

        // 3. Subtract from available balance
        let final_source_spendable =
            ops::subtract(&source_spendable_ct, &source_combined_ct).unwrap();

        // test
        let final_source_open =
            source_open - (open_lo.clone() + open_hi.clone() * Scalar::from(ops::TWO_32));
        let expected_source: pod::ElGamalCiphertext =
            source_pk.encrypt_with(22_u64, &final_source_open).into();
        assert_eq!(expected_source, final_source_spendable);

        // same for the destination account

        // 1. Combine commitments and handles
        let dest_lo_ct: pod::ElGamalCiphertext = (comm_lo, handle_dest_lo).into();
        let dest_hi_ct: pod::ElGamalCiphertext = (comm_hi, handle_dest_hi).into();

        // 2. Combine lo and hi ciphertexts
        let dest_combined_ct = ops::combine_lo_hi(&dest_lo_ct, &dest_hi_ct).unwrap();

        // 3. Add to pending balance
        let final_dest_pending = ops::add(&dest_pending_ct, &dest_combined_ct).unwrap();

        let final_dest_open = dest_open + (open_lo + open_hi * Scalar::from(ops::TWO_32));
        let expected_dest_ct: pod::ElGamalCiphertext =
            dest_pk.encrypt_with(132_u64, &final_dest_open).into();
        assert_eq!(expected_dest_ct, final_dest_pending);
    }
}
