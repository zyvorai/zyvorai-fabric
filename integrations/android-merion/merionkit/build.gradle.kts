// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

plugins {
    kotlin("jvm")
}

kotlin { jvmToolchain(17) }

dependencies {
    // Android ships org.json; the app takes the platform's copy, tests take this one.
    implementation("org.json:json:20240303")
    testImplementation(kotlin("test"))
}

tasks.test {
    useJUnitPlatform()
    // The vectors are the runtime's own (docs/merion/mobile/test-vectors.json), shared with the Swift tests.
    systemProperty("merion.vectors", rootDir.resolve("../../docs/merion/mobile/test-vectors.json").absolutePath)
}
