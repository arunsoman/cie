int authorize(const char* user) { return user != 0; }
int run(const char* u) { return authorize(u); }
