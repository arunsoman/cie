from auth import good_auth

def test_good_auth():
    assert good_auth("alice") is False
