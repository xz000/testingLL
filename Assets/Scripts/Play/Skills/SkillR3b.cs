﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillR3b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance = 6;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    public float SelfR = 0.51f;
    //MoveScript MS;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        //MS = GetComponent<MoveScript>();
    }

    // Update is called once per frame
    void GoSkillR3b()
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
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        Vector2 realplace;
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        gameObject.GetComponent<DoSkill>().Fire = null;
        Rigidbody2D selfrb2d = gameObject.GetComponent<Rigidbody2D>();
        Vector2 skilldirection = actionplace - selfrb2d.position;
        RaycastHit2D hit2D = Physics2D.Raycast(selfrb2d.position + skilldirection.normalized * SelfR, skilldirection - skilldirection.normalized * SelfR);
        if (hit2D.collider != null)
        {
            realplace = hit2D.point;
            if (hit2D.collider.GetComponent<GoWhereScript>() != null)
                hit2D.collider.GetComponent<GoWhereScript>().GoHere(transform.position);
            transform.position = realplace;
        }
        else
        {
            realplace = selfrb2d.position + maxdistance * skilldirection.normalized;
            transform.position = realplace;
        }
    }

    void SkillR3bSetLevel(int i)
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
