import sqlite3

con = sqlite3.connect("server/bdd.sqlite")
cur = con.cursor()

cur.execute("CREATE TABLE users(id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, name varchar(25) UNIQUE, email VARCHAR(100) UNIQUE, password VARCHAR(100))")