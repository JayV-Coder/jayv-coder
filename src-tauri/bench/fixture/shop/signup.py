def validate(user):
    """Returns the list of problems with a sign-up form."""
    problems = []
    if not user.get("name", "").strip():
        problems.append("name")
    return problems
