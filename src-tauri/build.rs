fn xml_escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn main() {
    println!("cargo:rerun-if-env-changed=DROPILOT_MSIX_PUBLISHER");

    let publisher = std::env::var("DROPILOT_MSIX_PUBLISHER")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "CN=Dropilot".to_string());
    let publisher_escaped = xml_escape_attr(&publisher);
    let manifest_version = format!("{}.0", env!("CARGO_PKG_VERSION"));

    let manifest = format!(
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="{manifest_version}" processorArchitecture="*" name="io.dropilot.app" type="win32" />
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="{publisher_escaped}" packageName="io.dropilot.app" applicationId="Dropilot" />
</assembly>"#,
    );

    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attrs).expect("failed to build tauri resources");
}
