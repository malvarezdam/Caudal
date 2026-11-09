plugins {
    id("com.android.application")
}

android {
    namespace = "app.caudal"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.caudal"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.games:games-activity:4.4.0")
}