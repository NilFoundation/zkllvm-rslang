// unit-test: DataflowConstProp

// EMIT_MIR tuple.main.DataflowConstProp.diff
fn main() {
    let mut a = (1, 2);
    let b = a.0 + a.1 + 3;
    a = (2, 3);
    let c = a.0 + a.1 + b;
}
