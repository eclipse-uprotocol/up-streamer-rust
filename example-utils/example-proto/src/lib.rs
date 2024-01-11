/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#[allow(non_snake_case)]
pub mod proto {
    // protoc-generated stubs, see build.rs
    pub mod google {
        pub mod r#type {

            include!(concat!(env!("OUT_DIR"), "/google.r#type.rs"));
        }
    }

    pub mod example {
        pub mod hello_world {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/example.hello_world.v1.rs"));
            }
        }
    }
}
