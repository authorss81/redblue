// Object: Bank Account
object BankAccount
    has owner
    has balance default 0
    has history
    
    to create(initial_balance)
        set this balance to initial_balance
        set this history to empty list
        give back this
    end
    
    to deposit(amount)
        if amount is less than 0
            say "Cannot deposit negative amount"
            give back nothing
        end
        
        add amount to this balance
        add "Deposited {amount}" to this history
        give back this balance
    end
    
    to withdraw(amount)
        if amount is greater than this balance
            say "Insufficient funds"
            give back nothing
        end
        
        subtract amount from this balance
        add "Withdrew {amount}" to this history
        give back this balance
    end
    
    to show_balance()
        say "{this owner}'s balance: ${this balance}"
    end
end

// Object: Savings Account (extends BankAccount)
object SavingsAccount extends BankAccount
    has interest_rate
    
    to create(initial_balance, rate)
        this.create(initial_balance)
        set this interest_rate to rate
    end
    
    to apply_interest()
        set interest to this balance * this interest_rate
        this.deposit(interest)
        add "Interest applied: {interest}" to this history
    end
end

// Usage
set account to new SavingsAccount
set owner of account to "Alice"
account.deposit(1000)
account.withdraw(50)
account.apply_interest()
account.show_balance()

// Show history
for each entry in account history
    say entry
end
