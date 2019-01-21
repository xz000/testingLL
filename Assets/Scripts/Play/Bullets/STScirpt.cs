using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class STScirpt : MonoBehaviour
{
    private float pasttime;
    public float maxtime = 2;
    public float BulletSpeed = 6;
    public GameObject fireball;
    //public Rigidbody2D selfRB2D;
    public GameObject sender;
    public Fix64Vector2 finalv;

    void FixedUpdate()
    {
        pasttime += Time.fixedDeltaTime;
        if (pasttime >= maxtime)
        {
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
    }

    private void OnCollisionEnter2D(Collision2D collision)
    {
        if (GetComponent<DestroyScript>().selfprotect && collision.gameObject == sender)
            return;
        GetComponent<DestroyScript>().Destroyself();
    }

    private void OnTriggerEnter2D(Collider2D collision)
    {
        finalv = (Fix64Vector2)gameObject.GetComponent<Rigidbody2D>().velocity;
    }

    private void OnDestroy()
    {
        FFF(finalv);
    }

    void FFF(Fix64Vector2 direction)
    {
        direction = direction.CCWTurn(-Fix64.Pi / (Fix64)10).normalized() * (Fix64)0.4;
        fireball.GetComponent<SABulletScript>().sender = null;
        Fix64Vector2 spv2 = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        for (int bnum = 0; bnum < 8; bnum++)
        {
            DoFire((spv2 + direction).ToV2(), (direction.normalized()*(Fix64)BulletSpeed).ToV2());
            direction = direction.CCWTurn(Fix64.Pi / (Fix64)40).normalized();
        }
    }

    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet = Instantiate(fireball, fireplace, Quaternion.identity);
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
    }

    private void OnCollisionExit2D(Collision2D collision)
    {
        gameObject.GetComponent<DestroyScript>().selfprotect = false;
    }
}
