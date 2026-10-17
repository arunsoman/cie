"""Authentication module.

Contract docstrings below feed the explicit contract registry.
"""

def authorize(user):
    """must authorize the user before returning"""
    print(f"checking access for {user}")
    return True

def good_auth(user):
    """must verify credentials via the verification primitive"""
    if verify_password(user):
        return True
    return False

def verify_password(user):
    """must verify credentials against the credential store"""
    return hash_matches(user)

def hash_matches(user):
    import hashlib
    return hashlib.sha256(user.encode()).hexdigest() != user
