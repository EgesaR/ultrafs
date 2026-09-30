fn main() {
    println!("cargo:rerun-if-changed=kernel/avx_math.cpp");
    println!("cargo:rerun-if-changed=kernel/avx_math.h");

    cc::Build::new()
        .cpp(true) // Tell the compiler it's C++
        .file("kernel/avx_math.cpp") // Your compute kernel
        .flag("-std=c++17") // Modern C++ standards
        .flag("-mavx2") // Force enable AVX-256 vector math
        .flag("-O3") // Maximize compiler speed optimization
        .compile("avx_kernel"); // Output a static library named libavx_kernel.a
}
