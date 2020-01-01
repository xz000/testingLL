﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class TestSkill01 : MonoBehaviour
{
    //public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed = 5;
    public GameObject fireball;
    public float force = 15;
    public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    public void GoTestSkill01()
    {
        Debug.Log("Wanna Go TestSkill01");
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
            Debug.Log("TestSkill01 Reached");
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        GetComponent<DoSkill>().BeforeSkill();
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + (Fix64)0.76 * skilldirection.normalized(), skilldirection.normalized() * (Fix64)bulletspeed);
        currentcooldown = 0;
        skillavaliable = false;
    }

    void DoFire(Fix64Vector2 fireplace, Fix64Vector2 speed2d)
    {
        GameObject bullet;
        fireball.GetComponent<BombExplode>().sender = gameObject;
        bullet = Instantiate(fireball, fireplace.ToV2(), Quaternion.identity);
        //bullet.GetComponent<BombExplode>().bombpower = force;
        bullet.GetComponent<BombExplode>().bombdamage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d.ToV2();
        //bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void TestSkill01SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
