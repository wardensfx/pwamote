# Vérifie que chaque fichier porte une signature Authenticode valide et horodatée.
# Usage : ./scripts/verify-signature.ps1 <fichier> [<fichier> ...]
$ErrorActionPreference = 'Stop'
$failed = $false
foreach ($path in @($args | ForEach-Object { $_ })) {
    $sig = Get-AuthenticodeSignature -FilePath $path
    $signer = $sig.SignerCertificate.Subject
    if ($sig.Status -ne 'Valid') {
        Write-Host "::error::$path : signature $($sig.Status) ($($sig.StatusMessage))"
        $failed = $true
    } elseif ($null -eq $sig.TimeStamperCertificate) {
        Write-Host "::error::$path : signé par $signer mais sans horodatage"
        $failed = $true
    } else {
        Write-Host "$path : signé par $signer"
    }
}
if ($failed) { exit 1 }
