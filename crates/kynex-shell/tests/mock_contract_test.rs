//! Mock contract verification test.
//! Compares `QMetaObject` introspection of real CXX-Qt models against QML test mocks.

#[test]
fn test_mock_contracts_match_real_models() {
    let (passed, report) = kynex_shell::shim::verify_mock_contracts();
    assert!(passed, "Mock contract audit failed:\n{report}");
}
