<?php
function authorize($user) { return $user !== null; }
function run($u) { return authorize($u); }
