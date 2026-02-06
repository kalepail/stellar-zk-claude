fn main() {
    if cfg!(feature = "cycle-prof") {
        use std::collections::HashMap;
        use risc0_build::{embed_methods_with_options, GuestOptionsBuilder};
        let opts = GuestOptionsBuilder::default()
            .features(vec!["cycle-prof".to_string()])
            .build()
            .unwrap();
        let mut map = HashMap::new();
        map.insert("asteroids-verify", opts);
        embed_methods_with_options(map);
    } else {
        risc0_build::embed_methods();
    }
}
