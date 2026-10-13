// Roleplay Bank: money every player keeps between visits and restarts,
// filed under their lasting id (`uid`) in the host's data for this
// Add-On. Other Add-Ons (a shop, a job, a tax) never touch the money
// themselves: they call `balance`, `pay` and `give`, and hear `paid` and
// `given`.

// What a new account starts with.
fn start() { 50 }

fn accounts() {
    let all = host_data("accounts");
    if all == () { #{} } else { all }
}

fn balance_of(p) {
    let money = accounts()[player(p).uid];
    if money == () { start() } else { money }
}

fn set_balance(p, money) {
    let all = accounts();
    all[player(p).uid] = money;
    set_host_data("accounts", all);
}

fn amount_ok(amount) {
    if type_of(amount) != "i64" || amount <= 0 {
        throw `an amount is a whole number above 0, not ${amount}`;
    }
}

// How much a player has.
fn api_balance(from, p) { balance_of(p) }

// Take money from a player for `from`'s sake. Throws, so the caller's whole
// call is undone, when they cannot afford it.
fn api_pay(from, p, amount) {
    amount_ok(amount);
    let money = balance_of(p);
    if amount > money {
        throw `${player(p).name} has $${money}, not $${amount}`;
    }
    set_balance(p, money - amount);
    emit("paid", #{ player: p, amount: amount, to: from });
    money - amount
}

// Give a player money (wages, a sale, a refund).
fn api_give(from, p, amount) {
    amount_ok(amount);
    let money = balance_of(p) + amount;
    set_balance(p, money);
    emit("given", #{ player: p, amount: amount, by: from });
    money
}

fn cmd_money(p) {
    tell(p, `You have $${balance_of(p)}.`);
}

fn cmd_grant(p, who, amount) {
    api_give("sample-rp-bank", who, amount);
    tell(p, `Gave ${player(who).name} $${amount}.`);
}
