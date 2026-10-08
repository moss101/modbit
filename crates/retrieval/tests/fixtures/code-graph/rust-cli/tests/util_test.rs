use taskcli::util::slug;

#[test]
fn slugs() {
    assert_eq!(slug("A B"), "a-b");
}
