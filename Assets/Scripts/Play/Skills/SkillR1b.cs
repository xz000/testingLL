﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillR1b : MonoBehaviour
{

    public CooldownImage MyImageScript;
    public float maxdistance = 5;
    private float currentcooldown;
    public float cooldowntime = 6;
    public bool skillavaliable;
    private float scd;
    public float scdtime = 2;
    bool doscd = false;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillR1b()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
        else if (doscd)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skillscd;
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
            if (doscd)
            {
                scd += Time.fixedDeltaTime;
                if (scd >= scdtime)
                    doscd = false;
            }
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        float realdistance = Mathf.Min(skilldirection.magnitude, maxdistance);
        if (realdistance <= 0.51)
        {
            return;
        }   //半径小于自身半径时不施法
        else
        {
            Vector2 realplace = singplace + skilldirection.normalized * realdistance;
            GetComponent<DoSkill>().BeforeSkill();
            gameObject.GetComponent<MoveScript>().controllable = true;
            currentcooldown = 0;
            skillavaliable = false;
            transform.position = realplace;
            doscd = true;
            scd = 0;
        }
    }

    public void Skillscd(Fix64Vector2 actionplacef)
    {
        if (!doscd)
            return;
        Vector2 actionplace = actionplacef.ToV2();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        float realdistance = Mathf.Min(skilldirection.magnitude, 4);
        if (realdistance <= 0.51)
        {
            return;
        }   //半径小于自身半径时不施法
        else
        {
            Vector2 realplace = singplace + skilldirection.normalized * realdistance;
            gameObject.GetComponent<MoveScript>().stopwalking(); //停止走动
            gameObject.GetComponent<StealthScript>().StealthEnd();
            gameObject.GetComponent<MoveScript>().controllable = true;
            transform.position = realplace;
            doscd = false;
        }
    }

    void SkillR1bSetLevel(int i)
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
