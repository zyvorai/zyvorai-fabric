// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

rootProject.name = "android-merion"

pluginManagement {
    repositories {
        gradlePluginPortal()
        google()
        mavenCentral()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

// The core library is plain JVM and builds anywhere there is a JDK. The app needs the Android SDK.
include(":merionkit")
if (System.getenv("ANDROID_HOME") != null || file("local.properties").exists()) {
    include(":app")
}
