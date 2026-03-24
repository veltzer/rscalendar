#!/bin/bash -eu
rscalendar copy-all-events --source "Clients - AgileSparks" \
	--target "Teaching" \
	--property-key "client" \
	--property-value "AgileSparks" \
	--dry-run
