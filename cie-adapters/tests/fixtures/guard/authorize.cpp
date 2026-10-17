bool authorize(const char* user) { return user != nullptr; }
bool run(const char* u) { return authorize(u); }
