interface Gate {
    fun authorize(user: String): Boolean
}

class Authorize : Gate {
    override fun authorize(user: String): Boolean { return user != null }
    fun run(u: String): Boolean { authorize(u); return true }
}
