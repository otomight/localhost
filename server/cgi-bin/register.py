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
        with open("server/templates/register.html", "r") as file :
            content = file.read()
            result = {
                "body": content
            }
            print(json.dumps(result))
            file.close()

    case "POST":
        # Parse body
        parsed_body = {}
        for val in body.split("\r\n"):
            if len(val) > 0:
                entry = val.split("=", 1)
                parsed_body[entry[0]] = entry[1]

        if parsed_body["password"] == parsed_body["confirm_password"]:
            # requete bdd
            cur.execute("INSERT INTO users(name, email, password) VALUES (?, ?, ?)", (
                parsed_body["username"],
                parsed_body["email"],
                hashlib.sha256(str(parsed_body["password"]).encode('utf-8')).hexdigest()))
            con.commit()

            # print resultat
            result = {
                "headers": {
                    "Location": "test.py"
                },
                "status": 303,
                "body": "OK",
            }
            print(json.dumps(result))

        else:
            print(json.dumps({"error": [500, "Something went wrong"]}))