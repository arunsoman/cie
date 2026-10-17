"""Account UI endpoints."""

def delete_account(request):
    user_id = request.args.get("id")
    cursor.execute(f"DELETE FROM users WHERE id = {user_id}")
    print("deleted account")
