import sys
import json
import sqlite3
import hashlib

con = sqlite3.connect("server/bdd.sqlite")
cur = con.cursor()
path = sys.argv[0]
method = sys.argv[1]
body = sys.argv[2]

match method:
    case "GET":
        with open("server/templates/login.html", "r") as file :
            content = file.read()
            result = {
                "body": content
            }
            print(json.dumps(result))
            file.close()

    case "POST":
        parsed_body = {}
        for val in body.split("\r\n"):
            if len(val) > 0:
                entry = val.split("=", 1)
                parsed_body[entry[0]] = entry[1]

        print(parsed_body)
        # requete bdd
        res = cur.execute("SELECT * FROM users WHERE email = ? AND password = ?", (
            parsed_body["email"],
            hashlib.sha256(str(parsed_body["password"]).encode('utf-8')).hexdigest())
            ).fetchall()

        if len(res) > 0 :
            # print resultat
            result = {
                "headers": {
                    "Status": 303,
                    "Location": "/test.py"
                },
                "body": parsed_body,
            }
            print(json.dumps(result))

        else:
            print(json.dumps({"error":[500, "Something went wrong"]}))
