// The macro defines 'f()' at the place where macro is called.
// This code is located at the place of call and two closures deep.
// Strip away this useless suffix.
pub(crate) const USELESS_SCOPE_NAME_SUFFIX: &str = "::{{closure}}::{{closure}}::f";

#[doc(hidden)]
#[inline(never)]
pub fn clean_function_name(name: &str) -> String {
    let Some(name) = name.strip_suffix(USELESS_SCOPE_NAME_SUFFIX) else {
        // Probably the user registered a user scope name.
        return name.to_owned();
    };
    shorten_rust_function_name(name)
}

/// Shorten a rust function name by removing the leading parts of module paths.
///
/// While the puffin profiling macros takes care of this internally, this function can be
/// useful for those registering custom scopes for rust functions.
///
/// # Example
/// ```
/// use puffin::shorten_rust_function_name;
///
/// assert_eq!(shorten_rust_function_name("foo::bar::baz::function_name"), "baz::function_name");
/// assert_eq!(shorten_rust_function_name("<some::ConcreteType as some::Trait>::function_name"), "<ConcreteType as Trait>::function_name");
/// ```
pub fn shorten_rust_function_name(name: &str) -> String {
    // "foo::bar::baz" -> "baz"
    fn last_part(name: &str) -> &str {
        if let Some(colon) = name.rfind("::") {
            &name[colon + 2..]
        } else {
            name
        }
    }

    // look for:  <some::ConcreteType as some::Trait>::function_name
    if let Some(end_caret) = name.rfind('>') {
        if let Some(trait_as) = name.rfind(" as ") {
            if trait_as < end_caret {
                let concrete_name = if let Some(start_caret) = name[..trait_as].rfind('<') {
                    &name[start_caret + 1..trait_as]
                } else {
                    name
                };

                let trait_name = &name[trait_as + 4..end_caret];

                let concrete_name = last_part(concrete_name);
                let trait_name = last_part(trait_name);

                let dubcolon_function_name = &name[end_caret + 1..];
                return format!("<{concrete_name} as {trait_name}>{dubcolon_function_name}");
            }
        }
    }

    if let Some(colon) = name.rfind("::") {
        if let Some(colon) = name[..colon].rfind("::") {
            // "foo::bar::baz::function_name" -> "baz::function_name"
            name[colon + 2..].to_owned()
        } else {
            // "foo::function_name" -> "foo::function_name"
            name.to_owned()
        }
    } else {
        name.to_owned()
    }
}

/// Shortens a long `file!()` path to the essentials.
///
/// We want to keep it short for two reasons: readability, and bandwidth
#[doc(hidden)]
#[inline(never)]
pub fn short_file_name(path: &str) -> String {
    if path.is_empty() {
        return "".to_string();
    }

    let path = path.replace('\\', "/"); // Handle Windows
    let components: Vec<&str> = path.split('/').collect();
    if components.len() <= 2 {
        return path;
    }

    // Look for `src` folder:

    let mut src_idx = None;
    for (i, c) in components.iter().enumerate() {
        if *c == "src" {
            src_idx = Some(i);
        }
    }

    if let Some(src_idx) = src_idx {
        // Before `src` comes the name of the crate - let's include that:
        let crate_index = src_idx.saturating_sub(1);
        let file_index = components.len() - 1;

        if crate_index + 2 == file_index {
            // Probably "crate/src/lib.rs" - include it all
            format!(
                "{}/{}/{}",
                components[crate_index],
                components[crate_index + 1],
                components[file_index]
            )
        } else if components[file_index] == "lib.rs" {
            // "lib.rs" is very unhelpful - include folder name:
            let folder_index = file_index - 1;

            if crate_index + 1 == folder_index {
                format!(
                    "{}/{}/{}",
                    components[crate_index], components[folder_index], components[file_index]
                )
            } else {
                // Ellide for brevity:
                format!(
                    "{}/…/{}/{}",
                    components[crate_index], components[folder_index], components[file_index]
                )
            }
        } else {
            // Ellide for brevity:
            format!("{}/…/{}", components[crate_index], components[file_index])
        }
    } else {
        // No `src` directory found - could be an example (`examples/hello_world.rs`).
        // Include the folder and file name.
        let n = components.len();
        // NOTE: we've already checked that n > 1 easily in the function
        format!("{}/{}", components[n - 2], components[n - 1])
    }
}

#[doc(hidden)]
#[inline(always)]
pub fn type_name_of<T>(_: T) -> &'static str {
    std::any::type_name::<T>()
}

#[test]
fn test_short_file_name() {
    for (before, after) in [
        ("", ""),
        ("foo.rs", "foo.rs"),
        ("foo/bar.rs", "foo/bar.rs"),
        ("foo/bar/baz.rs", "bar/baz.rs"),
        ("crates/cratename/src/main.rs", "cratename/src/main.rs"),
        ("crates/cratename/src/module/lib.rs", "cratename/…/module/lib.rs"),
        ("workspace/cratename/examples/hello_world.rs", "examples/hello_world.rs"),
        ("/rustc/d5a82bbd26e1ad8b7401f6a718a9c57c96905483/library/core/src/ops/function.rs", "core/…/function.rs"),
        ("/Users/emilk/.cargo/registry/src/github.com-1ecc6299db9ec823/tokio-1.24.1/src/runtime/runtime.rs", "tokio-1.24.1/…/runtime.rs"),
        ]
        {
        assert_eq!(short_file_name(before), after);
    }
}

#[test]
fn test_clean_function_name() {
    assert_eq!(clean_function_name(""), "");
    assert_eq!(
        clean_function_name(&format!("foo{}", USELESS_SCOPE_NAME_SUFFIX)),
        "foo"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar{}", USELESS_SCOPE_NAME_SUFFIX)),
        "foo::bar"
    );
    assert_eq!(
        clean_function_name(&format!("foo::bar::baz{}", USELESS_SCOPE_NAME_SUFFIX)),
        "bar::baz"
    );
    assert_eq!(
        clean_function_name(&format!(
            "some::GenericThing<_, _>::function_name{}",
            USELESS_SCOPE_NAME_SUFFIX
        )),
        "GenericThing<_, _>::function_name"
    );
    assert_eq!(
        clean_function_name(&format!(
            "<some::ConcreteType as some::bloody::Trait>::function_name{}",
            USELESS_SCOPE_NAME_SUFFIX
        )),
        "<ConcreteType as Trait>::function_name"
    );
}
