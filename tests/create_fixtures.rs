#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    use airplay2::plist_dict;
    use airplay2::protocol::plist::{self, PlistValue};

    #[test]
    fn generate_plist_fixtures() {
        let fixtures_dir = Path::new("tests/fixtures");
        if !fixtures_dir.exists() {
            fs::create_dir_all(fixtures_dir).unwrap();
        }

        // 1. Simple Dictionary
        let simple_dict = plist_dict! {
            "key" => "value",
            "int" => 42i64,
            "bool" => true,
        };
        save_fixture(fixtures_dir, "simple_dict.bplist", &simple_dict);

        // 2. Nested Dictionary
        let nested_dict = plist_dict! {
            "parent" => plist_dict! {
                "child" => "hello",
                "grandchild" => 123i64
            }
        };
        save_fixture(fixtures_dir, "nested_dict.bplist", &nested_dict);

        // 3. Array
        let array = PlistValue::Array(vec![
            PlistValue::Integer(1),
            PlistValue::String("two".to_string()),
            PlistValue::Boolean(false),
        ]);
        save_fixture(fixtures_dir, "array.bplist", &array);

        // 4. Large Dictionary
        let mut large_map = HashMap::new();
        for i in 0..100 {
            large_map.insert(format!("key_{i}"), PlistValue::Integer(i));
        }
        let large_dict = PlistValue::Dictionary(large_map);
        save_fixture(fixtures_dir, "large_dict.bplist", &large_dict);

        // 5. Data Types
        let mut data_map = HashMap::new();
        data_map.insert(
            "data".to_string(),
            PlistValue::Data(vec![0xCA, 0xFE, 0xBA, 0xBE]),
        );
        data_map.insert("date".to_string(), PlistValue::Date(0.0)); // 2001-01-01
        #[allow(clippy::approx_constant)]
        data_map.insert("real".to_string(), PlistValue::Real(3.14159));
        let types_dict = PlistValue::Dictionary(data_map);
        save_fixture(fixtures_dir, "types.bplist", &types_dict);

        println!("Fixtures generated in tests/fixtures/");
    }

    fn save_fixture(dir: &Path, name: &str, value: &PlistValue) {
        let encoded = plist::encode(value).expect("Failed to encode plist");
        fs::write(dir.join(name), encoded).expect("Failed to write fixture");
    }
}
