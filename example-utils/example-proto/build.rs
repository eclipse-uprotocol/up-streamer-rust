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

use prost_build::Config;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> std::io::Result<()> {
    // use vendored protoc instead of relying on user provided protobuf installation
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    if let Err(err) = get_and_build_protos(
        &[
            "https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/descriptor.proto",
            "https://raw.githubusercontent.com/googleapis/googleapis/master/google/type/timeofday.proto",
            "https://raw.githubusercontent.com/eclipse-uprotocol/uprotocol-core-api/4334aff1650e491c06ae0eaef2bf68d6ed65fd3f/uprotocol/uprotocol_options.proto",
            "https://raw.githubusercontent.com/COVESA/uservices/main/src/main/proto/example/hello_world/v1/hello_world_topics.proto",
            "https://raw.githubusercontent.com/COVESA/uservices/main/src/main/proto/example/hello_world/v1/hello_world_service.proto",
        ]
    ) {
        let error_message = format!("Failed to fetch and build protobuf file: {err:?}");
        return Err(std::io::Error::new(std::io::ErrorKind::Other, error_message));
    }

    Ok(())
}

// Fetch protobuf definitions from `url`, and build them with prost_build
fn get_and_build_protos(urls: &[&str]) -> core::result::Result<(), Box<dyn std::error::Error>> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let mut proto_files = Vec::new();

    for url in urls {
        let file_name = url.split('/').last().unwrap();
        let mut file_path_buf = PathBuf::from(&out_dir);

        // Check if the URL is from googleapis to determine the correct path
        if url.contains("googleapis/googleapis") {
            file_path_buf.push("google/type");
        }

        if url.contains("protocolbuffers/protobuf") {
            file_path_buf.push("google/protobuf");
        }

        if url.contains("example/hello_world/v1") {
            file_path_buf.push("example/hello_world/v1")
        }

        file_path_buf.push(file_name); // Push the file name to the path buffer

        // Create the directory path if it doesn't exist
        if let Some(parent) = file_path_buf.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download the .proto file
        if let Err(err) = download_and_write_file(url, &file_path_buf) {
            panic!("Failed to download and write file: {err:?}");
        }

        proto_files.push(file_path_buf);
    }

    // Compile all .proto files together
    let mut config = Config::new();
    config.disable_comments(["."]);

    // Use references to PathBuf directly
    config.compile_protos(&proto_files, &[&PathBuf::from(out_dir)])?;

    Ok(())
}

fn download_and_write_file(
    url: &str,
    destination: &Path,
) -> core::result::Result<(), Box<dyn std::error::Error>> {
    // Send a GET request to the URL
    let resp = ureq::get(url).call();

    match resp {
        Err(error) => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            error.to_string(),
        ))),
        Ok(response) => {
            // Ensure parent directories exist
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }

            // Open a file in write-only mode
            let mut out_file = fs::File::create(destination)?;

            // Write the response body directly to the file
            let content = response.into_string()?;
            out_file.write_all(content.as_bytes())?;

            Ok(())
        }
    }
}
