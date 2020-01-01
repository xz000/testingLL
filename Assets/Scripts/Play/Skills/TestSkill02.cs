﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class TestSkill02 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    //MoveScript MS;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        //MS = GetComponent<MoveScript>();
    }

    public void GoTestSkill02()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
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
        //DoSkill.singing = 0; //停止吟唱中技能
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        Fix64 md64 = (Fix64)maxdistance;
        Fix64 skdl = skilldirection.Length();
        Fix64 realdistance;
        if (skdl <= md64)
            realdistance = skdl;
        else
            realdistance = md64;
        if (realdistance <= (Fix64)0.51)
        {
            return;
        }   //半径小于自身半径时不施法
        else
        {
            //gameObject.GetComponent<DoSkill>().Fire = null;
            Fix64Vector2 realplace = singplace + skilldirection.normalized() * realdistance;
            Rigidbody2D selfrb2d = gameObject.GetComponent<Rigidbody2D>();
            GetComponent<DoSkill>().BeforeSkill();
            //MS.controllable = true;
            currentcooldown = 0;
            skillavaliable = false;
            Vector2 rpv2 = realplace.ToV2();
            if (Physics2D.OverlapPoint(rpv2))
            {
                Collider2D hit = Physics2D.OverlapPoint(rpv2);
                HPScript hps = hit.GetComponent<HPScript>();
                if (hps != null)
                {
                    Rigidbody2D rb2d = hit.GetComponent<Rigidbody2D>();
                    selfrb2d.position = rb2d.position;
                    hps.TransferTo(singplace.ToV2());
                }
                else
                {
                    transform.position = rpv2;
                }
            }
            else
            {
                transform.position = rpv2;
            }
        }
    }

    void TestSkill02SetLevel(int i)
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

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iR.Fif = CalcFA;
    }
}
