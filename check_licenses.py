import json
import subprocess

def main():
    output = subprocess.check_output(["cargo", "metadata", "--format-version", "1"])
    metadata = json.loads(output)

    licenses = set()
    package_licenses = []

    for pkg in metadata["packages"]:
        lic = pkg.get("license", "N/A")
        package_licenses.append(f"{pkg['name']} {pkg['version']}: {lic}")
        if lic:
            # Split OR/AND licenses
            parts = lic.replace(" OR ", "/").replace(" AND ", "/").split("/")
            for p in parts:
                licenses.add(p.strip())
        else:
            licenses.add("N/A")

    print("--- Detected Licenses ---")
    for l in sorted(licenses):
        print(l)

    print("\n--- Package Licenses ---")
    for l in package_licenses:
        if "MIT" not in l and "Apache-2.0" not in l and "BSD" not in l:
             print(l)

if __name__ == "__main__":
    main()
