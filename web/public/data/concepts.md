# Hierarchical Concept Behavior

## Project (Domain)
- Symbols: auth.authorize, auth.good_auth, auth.verify_password, auth.hash_matches, payments.charge, payments.list_purchases, payments.refund, payments.notify_user, test_auth.test_good_auth, ui.delete_account
  - Behavior: Authorize
  - Behavior: Authorize Good Auth
  - Behavior: Verify Password
  - Behavior: Hash Matches
  - Behavior: Move money via Charge
  - Behavior: Query data via List Purchases
  - Behavior: Move money via Refund
  - Behavior: Notify User
  - Behavior: Authorize Test Good Auth
  - Behavior: Mutate data via Delete Account
  - Rule: C-000 (must_authorize, explicit)
  - Rule: C-001 (must_not_leak_secrets, explicit)
  - Rule: C-002 (must_not_leak_secrets, explicit)
  - Rule: C-003 (must_authorize, inferred_candidate)
  - Rule: C-004 (must_authorize, inferred_candidate)
  - Rule: C-005 (must_authorize, inferred_candidate)
  - Rule: C-006 (must_authorize, inferred_candidate)
  - Rule: C-007 (must_authorize, inferred_candidate)
  - Rule: C-008 (must_authorize, inferred_candidate)
- Coverage: 100% complete=true confidence=0.60
  ## auth.py (Module)
  - Symbols: auth.authorize, auth.good_auth, auth.verify_password, auth.hash_matches
    - Behavior: Authorize
    - Behavior: Authorize Good Auth
    - Behavior: Verify Password
    - Behavior: Hash Matches
    - Rule: C-000 (must_authorize, explicit)
    - Rule: C-001 (must_not_leak_secrets, explicit)
    - Rule: C-002 (must_not_leak_secrets, explicit)
    - Rule: C-003 (must_authorize, inferred_candidate)
    - Rule: C-004 (must_authorize, inferred_candidate)
    - Rule: C-005 (must_authorize, inferred_candidate)
  - Coverage: 100% complete=true confidence=0.60
    ## Concept:auth:authorization (UseCase) *(also under: auth.py [cluster_cohesion])*
    - Intent: auth
    - Symbols: auth.authorize, auth.good_auth, auth.verify_password
    - Coverage: 100% complete=true confidence=0.80
    ## Concept:crypto:uses crypto (UseCase) *(also under: auth.py [cluster_cohesion])*
    - Intent: crypto
    - Symbols: auth.good_auth, auth.verify_password, auth.hash_matches
    - Coverage: 100% complete=true confidence=0.80
    ## Concept:auth+crypto:authorization & uses crypto (UseCase) *(also under: auth.py [cluster_cohesion])*
    - Intent: auth, crypto
    - Symbols: auth.good_auth, auth.verify_password
    - Coverage: 100% complete=true confidence=0.80
    ## must authorize the user before returning (Rule)
    - Symbols: auth.authorize
      - Rule: C-000 (must_authorize, explicit)
    - Coverage: 100% complete=true confidence=1.00
    ## must verify credentials via the verification primitive (Rule)
    - Symbols: auth.good_auth
      - Rule: C-001 (must_not_leak_secrets, explicit)
    - Coverage: 100% complete=true confidence=1.00
    ## must verify credentials against the credential store (Rule)
    - Symbols: auth.verify_password
      - Rule: C-002 (must_not_leak_secrets, explicit)
    - Coverage: 100% complete=true confidence=1.00
    ## 'authorize' must perform real authorization before returning success (Invariant)
    - Symbols: auth.authorize
      - Rule: C-003 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
    ## 'good_auth' must perform real authorization before returning success (Invariant)
    - Symbols: auth.good_auth
      - Rule: C-004 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
    ## 'verify_password' must perform real authorization before returning success (Invariant)
    - Symbols: auth.verify_password
      - Rule: C-005 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
  ## payments.py (Module)
  - Symbols: auth.authorize, payments.charge, payments.list_purchases, payments.refund, payments.notify_user, ui.delete_account
    - Behavior: Move money via Charge
    - Behavior: Query data via List Purchases
    - Behavior: Move money via Refund
    - Behavior: Notify User
    - Rule: C-006 (must_authorize, inferred_candidate)
    - Rule: C-007 (must_authorize, inferred_candidate)
  - Coverage: 100% complete=true confidence=0.60
    ## Concept:print:prints (UseCase) *(also under: payments.py [cluster_cohesion])*
    - Intent: print
    - Symbols: auth.authorize, payments.charge, payments.refund, payments.notify_user, ui.delete_account
    - Coverage: 100% complete=true confidence=0.80
    ## Concept:money+print:money movement & prints (UseCase) *(also under: payments.py [cluster_cohesion])*
    - Intent: money, print
    - Symbols: payments.charge, payments.refund
    - Coverage: 100% complete=true confidence=0.80
    ## 'charge' must be preceded by established authorization (Invariant)
    - Symbols: payments.charge
      - Rule: C-006 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
    ## 'refund' must be preceded by established authorization (Invariant)
    - Symbols: payments.refund
      - Rule: C-007 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
  ## test_auth.py (Module)
  - Symbols: test_auth.test_good_auth
    - Behavior: Authorize Test Good Auth
  - Coverage: 100% complete=true confidence=1.00
  ## ui.py (Module)
  - Symbols: payments.charge, ui.delete_account
    - Behavior: Mutate data via Delete Account
    - Rule: C-008 (must_authorize, inferred_candidate)
  - Coverage: 100% complete=true confidence=0.60
    ## Concept:db Write+mutation+print:writes data & mutations & prints (UseCase) *(also under: ui.py [cluster_cohesion])*
    - Intent: db_write, mutation, print
    - Symbols: payments.charge, ui.delete_account
    - Coverage: 100% complete=true confidence=0.80
    ## 'delete_account' must be preceded by established authorization (Invariant)
    - Symbols: ui.delete_account
      - Rule: C-008 (must_authorize, inferred_candidate)
    - Coverage: 100% complete=true confidence=0.60
  ## Concept:print:prints (UseCase) *(also under: payments.py [cluster_cohesion])*
  - Intent: print
  - Symbols: auth.authorize, payments.charge, payments.refund, payments.notify_user, ui.delete_account
  - Coverage: 100% complete=true confidence=0.80
  ## Concept:auth:authorization (UseCase) *(also under: auth.py [cluster_cohesion])*
  - Intent: auth
  - Symbols: auth.authorize, auth.good_auth, auth.verify_password
  - Coverage: 100% complete=true confidence=0.80
  ## Concept:crypto:uses crypto (UseCase) *(also under: auth.py [cluster_cohesion])*
  - Intent: crypto
  - Symbols: auth.good_auth, auth.verify_password, auth.hash_matches
  - Coverage: 100% complete=true confidence=0.80
  ## Concept:auth+crypto:authorization & uses crypto (UseCase) *(also under: auth.py [cluster_cohesion])*
  - Intent: auth, crypto
  - Symbols: auth.good_auth, auth.verify_password
  - Coverage: 100% complete=true confidence=0.80
  ## Concept:db Write+mutation+print:writes data & mutations & prints (UseCase) *(also under: ui.py [cluster_cohesion])*
  - Intent: db_write, mutation, print
  - Symbols: payments.charge, ui.delete_account
  - Coverage: 100% complete=true confidence=0.80
  ## Concept:money+print:money movement & prints (UseCase) *(also under: payments.py [cluster_cohesion])*
  - Intent: money, print
  - Symbols: payments.charge, payments.refund
  - Coverage: 100% complete=true confidence=0.80

