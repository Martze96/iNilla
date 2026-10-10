//! Stamps the commit this library was built from, exactly as the `benilla` launcher shim does
//! (`benilla-buildstamp` owns the rule).

fn main() {
    benilla_buildstamp::emit();
}
