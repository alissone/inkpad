cacert.pem: Mozilla CA certificate bundle (MPL 2.0), from https://curl.se/docs/caextract.html.
Used by inkpad-net so HTTPS works even when the Kindle's own certificate store is old.
Refresh it now and then: curl -o cacert.pem https://curl.se/ca/cacert.pem
