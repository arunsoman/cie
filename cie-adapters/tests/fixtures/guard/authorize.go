package auth

func authorize(user string) bool { return user != "" }

func run(u string) bool { return authorize(u) }
