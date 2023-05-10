use crate::{
    assets::RelayAssetId,
    common_types::AssetId,
    constants::{StakingPotAccount, TreasuryAccount},
    AccountId, Balance, Runtime, RuntimeCall, RuntimeEvent, Tokens,
};
use codec::{Decode, Encode};
use frame_support::traits::{
    fungible::Inspect as FungibleInspect,
    fungibles::{Balanced, CreditOf},
    tokens::{BalanceConversion, WithdrawConsequence},
    Contains, OnUnbalanced,
};
use orml_tokens::CurrencyAdapter;
use pallet_asset_tx_payment::OnChargeAssetTransaction;
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{DispatchInfoOf, One, PostDispatchInfoOf, Zero},
    transaction_validity::{InvalidTransaction, TransactionValidityError},
};

pub struct KSMEnabledPallets;
impl Contains<RuntimeCall> for KSMEnabledPallets {
    fn contains(t: &RuntimeCall) -> bool {
        matches!(
            t,
            RuntimeCall::INV4(_)
                | RuntimeCall::Rings(_)
                | RuntimeCall::Tokens(_)
                | RuntimeCall::XTokens(_)
        )
    }
}

impl pallet_asset_tx_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Fungibles = Tokens;
    type OnChargeAssetTransaction = FilteredTransactionCharger;
}

pub struct TnkrToKsm;
impl BalanceConversion<Balance, AssetId, Balance> for TnkrToKsm {
    type Error = ();

    fn to_asset_balance(balance: Balance, asset_id: AssetId) -> Result<Balance, Self::Error> {
        if asset_id == 1u32 {
            Ok(balance.saturating_div(20u128))
        } else {
            return Err(());
        }
    }
}

pub struct FilteredTransactionCharger;
impl OnChargeAssetTransaction<Runtime> for FilteredTransactionCharger {
    type AssetId = AssetId;
    type Balance = Balance;
    type LiquidityInfo = CreditOf<AccountId, Tokens>;

    fn withdraw_fee(
        who: &AccountId,
        call: &RuntimeCall,
        _dispatch_info: &sp_runtime::traits::DispatchInfoOf<RuntimeCall>,
        asset_id: AssetId,
        fee: Balance,
        _tip: Balance,
    ) -> Result<CreditOf<AccountId, Tokens>, frame_support::unsigned::TransactionValidityError>
    {
        if KSMEnabledPallets::contains(call) && asset_id == 1u32 {
            let min_converted_fee = if fee.is_zero() {
                Zero::zero()
            } else {
                One::one()
            };

            let fee = TnkrToKsm::to_asset_balance(fee, asset_id)
                .map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?
                .max(min_converted_fee);

            let can_withdraw = CurrencyAdapter::<Runtime, RelayAssetId>::can_withdraw(who, fee);

            if !matches!(can_withdraw, WithdrawConsequence::Success) {
                return Err(InvalidTransaction::Payment.into());
            }

            <Tokens as Balanced<AccountId>>::withdraw(asset_id, who, fee)
                .map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))
        } else {
            Err(TransactionValidityError::from(InvalidTransaction::Payment))
        }
    }

    fn correct_and_deposit_fee(
        who: &AccountId,
        _dispatch_info: &DispatchInfoOf<RuntimeCall>,
        _post_info: &PostDispatchInfoOf<RuntimeCall>,
        corrected_fee: Balance,
        _tip: Balance,
        paid: CreditOf<AccountId, Tokens>,
    ) -> Result<(), TransactionValidityError> {
        let min_converted_fee = if corrected_fee.is_zero() {
            Zero::zero()
        } else {
            One::one()
        };

        let corrected_fee = TnkrToKsm::to_asset_balance(corrected_fee, paid.asset())
            .map_err(|_| TransactionValidityError::from(InvalidTransaction::Payment))?
            .max(min_converted_fee);

        let (final_fee, refund) = paid.split(corrected_fee);

        let _ = <Tokens as Balanced<AccountId>>::resolve(who, refund);

        DealWithKSMFees::on_unbalanced(final_fee);

        Ok(())
    }
}

pub struct DealWithKSMFees;
impl OnUnbalanced<CreditOf<AccountId, Tokens>> for DealWithKSMFees {
    fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = CreditOf<AccountId, Tokens>>) {
        if let Some(mut fees) = fees_then_tips.next() {
            if let Some(tips) = fees_then_tips.next() {
                // Merge with fee, for now we send everything to the treasury
                let _ = fees.subsume(tips);
            }

            Self::on_unbalanced(fees);
        }
    }

    fn on_unbalanced(amount: CreditOf<AccountId, Tokens>) {
        //let (to_collators, to_treasury) = amount.ration(50, 50);

        let total: u128 = 100u128;
        let amount1 = amount.peek().saturating_mul(50u128) / total;
        let (to_collators, to_treasury) = amount.split(amount1);

        let _ = <Tokens as Balanced<AccountId>>::resolve(&TreasuryAccount::get(), to_treasury);

        let _ = <Tokens as Balanced<AccountId>>::resolve(&StakingPotAccount::get(), to_collators);
    }
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct ChargerExtra {
    #[codec(compact)]
    pub tip: Balance,
    pub asset_id: Option<AssetId>,
}
