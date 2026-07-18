# connect-usb.ps1 - Windows-side helper for reaching pwnghost-rs over the
# USB-ethernet gadget link (dwc2 + g_ether, see pi-gen/stage1/00-boot-files
# and pi-gen/stage5's usb0.nmconnection).
#
# Even when the Pi's gadget interface enumerates correctly and
# NetworkManager brings up usb0 with ipv4.method=shared (which runs a real
# DHCP server for the connected host), Windows does not always request or
# keep a DHCP lease on a brand-new composite USB network adapter promptly.
# oxigotchi (a sibling Rust reimplementation) hits the same issue and ships
# a companion script (tools/setup_rndis_ip.ps1) for exactly this reason;
# this is our equivalent, generalized to try DHCP first before falling
# back to a static address.
#
# Run as: powershell -ExecutionPolicy Bypass -File connect-usb.ps1

$adapter = Get-NetAdapter | Where-Object {
    $_.InterfaceDescription -match 'RNDIS|Remote NDIS|USB Ethernet|CDC|Gadget|Raspberry'
} | Select-Object -First 1

if (-not $adapter) {
    Write-Host "No RNDIS/CDC-Ethernet gadget adapter found." -ForegroundColor Yellow
    Write-Host "Check Device Manager for an unrecognized/undriven USB network device," -ForegroundColor Yellow
    Write-Host "or confirm the Pi is on the data-capable USB port (not power-only) and its" -ForegroundColor Yellow
    Write-Host "cable actually carries USB data lines." -ForegroundColor Yellow
    exit 1
}

$name = $adapter.Name
Write-Host "Found gadget adapter: $name ($($adapter.InterfaceDescription))"

# Give DHCP (served by the Pi's NetworkManager usb0 profile) a chance first.
Write-Host "Renewing DHCP lease on $name..."
try { ipconfig /renew "$name" | Out-Null } catch {}
Start-Sleep -Seconds 2

$ip = Get-NetIPAddress -InterfaceAlias $name -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.IPAddress -like '10.0.0.*' -or $_.PrefixOrigin -eq 'Dhcp' }

if ($ip) {
    Write-Host "DHCP address already present: $($ip.IPAddress)" -ForegroundColor Green
} else {
    Write-Host "No DHCP lease yet -- assigning a static fallback address (10.0.0.10/24)."
    $existing = Get-NetIPAddress -InterfaceAlias $name -IPAddress "10.0.0.10" -ErrorAction SilentlyContinue
    if (-not $existing) {
        New-NetIPAddress -InterfaceAlias $name -IPAddress 10.0.0.10 -PrefixLength 24 -ErrorAction SilentlyContinue | Out-Null
        Write-Host "Added 10.0.0.10/24 to $name" -ForegroundColor Green
    } else {
        Write-Host "10.0.0.10/24 already on $name"
    }
}

Write-Host ""
Write-Host "Try: ssh pi@10.0.0.2  (password: raspberry, unless already changed)"
Write-Host "If that fails, also try: ssh pi@pwnghost.local"
