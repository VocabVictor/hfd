use std::fs;
use std::path::Path;
use std::env;

fn main() {
    let manifest_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    
    let mut version_parts = manifest
        .lines()
        .find(|line| line.trim().starts_with("version"))
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .trim()
        .trim_matches('"')
        .split('.')
        .map(|s| s.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    
    let counter = manifest
        .lines()
        .find(|line| line.trim().starts_with("build_counter"))
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .trim()
        .trim_matches('"')
        .parse::<u32>()
        .unwrap();
    
    version_parts[2] = counter + 1;
    
    let new_version = format!("{}.{}.{}", version_parts[0], version_parts[1], version_parts[2]);
    let new_manifest = manifest
        .replace(
            &format!("version = \"{}\"", version_parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".")),
            &format!("version = \"{}\"", new_version),
        )
        .replace(
            &format!("build_counter = \"{}\"", counter),
            &format!("build_counter = \"{}\"", counter + 1),
        );
    
    fs::write(&manifest_path, new_manifest).unwrap();
    
    // Update Python package versions
    let py_manifest_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("hfd-py")
        .join("Cargo.toml");
    let py_manifest = fs::read_to_string(&py_manifest_path).unwrap();
    let new_py_manifest = py_manifest.replace(
        &format!("version = \"{}\"", version_parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".")),
        &format!("version = \"{}\"", new_version),
    );
    fs::write(&py_manifest_path, new_py_manifest).unwrap();
    
    let pyproject_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("hfd-py")
        .join("pyproject.toml");
    let pyproject = fs::read_to_string(&pyproject_path).unwrap();
    let new_pyproject = pyproject.replace(
        &format!("version = \"{}\"", version_parts.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(".")),
        &format!("version = \"{}\"", new_version),
    );
    fs::write(&pyproject_path, new_pyproject).unwrap();
    
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=hfd-py/Cargo.toml");
    println!("cargo:rerun-if-changed=hfd-py/pyproject.toml");
} 