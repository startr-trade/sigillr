#!/bin/sh
# Regenerates the server key pinning fixtures of tests/pin.rs (FTR-004 T10, R-5):
#   pin_ca.cert.pem                        a fixture CA the test adds as a trust root
#   pin_a.cert.pem, pin_a.test-key.pem     server certificate A for localhost, and its key
#   pin_b.cert.pem, pin_b.test-key.pem     server certificate B for localhost, and its key
# Every key here is a test key. The CA key is thrown away once A and B are signed.
# After regenerating, update PIN_A in src/pin.rs and tests/pin.rs with the command there.
set -eu
cd "$(dirname "$0")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

days=36500

openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$tmp/ca.key"
openssl req -x509 -new -key "$tmp/ca.key" -sha256 -days "$days" \
    -subj "/CN=sigillr-client pin test CA" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -out pin_ca.cert.pem

cat > "$tmp/leaf.ext" <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost
EOF

for n in a b; do
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "pin_$n.test-key.pem"
    openssl req -new -key "pin_$n.test-key.pem" -subj "/CN=localhost" -out "$tmp/$n.csr"
    openssl x509 -req -in "$tmp/$n.csr" -CA pin_ca.cert.pem -CAkey "$tmp/ca.key" \
        -CAcreateserial -CAserial "$tmp/ca.srl" -sha256 -days "$days" \
        -extfile "$tmp/leaf.ext" -out "pin_$n.cert.pem"
done

for n in a b; do
    printf 'pin %s: sha256//' "$n"
    openssl x509 -in "pin_$n.cert.pem" -pubkey -noout | openssl pkey -pubin -outform der \
        | openssl dgst -sha256 -binary | base64
done
