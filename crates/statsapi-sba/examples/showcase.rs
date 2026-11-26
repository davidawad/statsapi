use statsapi_sba::{decode_loans, decode_ppp, Loan, PppLoan, Program};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut loans = Vec::new();
    decode_loans(
        &include_bytes!("../tests/fixtures/loans-7a.csv")[..],
        |loan| {
            loans.push(loan);
        },
    )?;
    let mut ppp = Vec::new();
    decode_ppp(&include_bytes!("../tests/fixtures/ppp.csv")[..], |loan| {
        ppp.push(loan);
    })?;

    println!("SBA synthetic sample");
    println!(
        "{:<8} {:<6} {:<8} {:>12}",
        "Program", "State", "ZIP", "Amount"
    );
    for loan in loans.iter().take(2) {
        let program = match loan.program {
            Program::SevenA => "7(a)",
            Program::FiveOhFour => "504",
            _ => "other",
        };
        print_loan(loan, program);
    }
    for loan in ppp.iter().take(2) {
        print_ppp(loan);
    }
    Ok(())
}

fn print_loan(loan: &Loan, program: &str) {
    println!(
        "{program:<8} {:<6} {:<8} {:>12.1}",
        loan.state, loan.zip, loan.gross_approval
    );
}

fn print_ppp(loan: &PppLoan) {
    println!(
        "{:<8} {:<6} {:<8} {:>12.1}",
        "PPP", loan.state, loan.zip, loan.amount
    );
}
