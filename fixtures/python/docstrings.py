"""
Module for handling payment processing.

This module provides the core payment gateway integration
with support for multiple providers.
"""

from decimal import Decimal
from typing import Optional


class PaymentProcessor:
    """Processes payments through various gateways.

    Supports Stripe, PayPal, and direct bank transfers.
    Implements retry logic and idempotency.
    """

    def process(self, amount: Decimal, currency: str = "USD") -> str:
        """Process a payment and return the transaction ID.

        Args:
            amount: The payment amount.
            currency: ISO 4217 currency code.

        Returns:
            A unique transaction identifier.

        Raises:
            PaymentError: If the payment fails.
        """
        validated_amount = self._validate_amount(amount)
        gateway = self._select_gateway(currency)
        result = gateway.charge(validated_amount, currency)
        self._record_transaction(result)
        return result.transaction_id
