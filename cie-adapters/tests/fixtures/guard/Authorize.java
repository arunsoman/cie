public class Authorize {
    public boolean authorize(String user) { return user != null; }
    public boolean run(String u) { authorize(u); return true; }
}
