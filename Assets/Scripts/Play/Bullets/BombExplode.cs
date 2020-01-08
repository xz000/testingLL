﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class BombExplode : MonoBehaviour
{
    public AudioClip SoundHit;
    private float pasttime;
    public float maxtime;
    public float pushtime = 1;
    public float bombpower;
    public float bombdamage;
    public GameObject sender;
    public bool selfbreak = true;
    //public bool selfprotect = true;

    // Use this for initialization
    void Start()
    {
        //selfprotect = true;
        pasttime = 0;
    }

    // Update is called once per frame
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
        /*if (gameObject.GetComponent<DestroyScript>().selfprotect && collision.gameObject == sender)
            return;*/
        /*if (selfprotect && collision.gameObject.GetComponent<ShieldScript>().sender == sender)
            return;*/
        Rigidbody2D rb = collision.gameObject.GetComponent<Rigidbody2D>();
        HPScript hp = collision.gameObject.GetComponent<HPScript>();
        if (hp != null && rb != null)
        {
            Fix64Vector2 explforce;
            Rigidbody2D selfrb = gameObject.GetComponent<Rigidbody2D>();
            explforce = (Fix64Vector2)rb.position - (Fix64Vector2)selfrb.position;
            collision.gameObject.GetComponent<RBScript>().GetPushed(explforce.normalized() * (Fix64)bombpower, pushtime);
            //hp.GetKicked(explforce.normalized * bombpower);
            hp.GetHurt(bombdamage);
        }
        if (selfbreak)
        {
            AudioSource.PlayClipAtPoint(SoundHit, rb.position);
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
    }
}
