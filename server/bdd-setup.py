import sqlite3

con = sqlite3.connect("server/bdd.sqlite")
cur = con.cursor()

cur.execute("CREATE TABLE users(id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name varchar(25) UNIQUE, email VARCHAR(100) UNIQUE, password VARCHAR(100))")
cur.execute("CREATE TABLE session(id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, uuid VARCHAR NOT NULL, FOREIGN KEY(user_id) REFERENCES users(id))")