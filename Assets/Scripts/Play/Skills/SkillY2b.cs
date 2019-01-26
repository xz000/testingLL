using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillY2b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject MySuiteObj;
    public GameObject MySuite;
    public float maxdistance = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    //public float maxtime;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillY2b()
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

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        Fix64Vector2 sn = skilldirection.normalized();
        Fix64 md = (Fix64)maxdistance;
        Fix64 sl = skilldirection.Length();
        Fix64 realdistance;
        if (sl >= md)
            realdistance = md + (Fix64)2;
        else
            realdistance = sl + (Fix64)2;
        if (realdistance <= (Fix64)2.5)
        {
            return;
        }   //半径小于自身半径时不施法
        GetComponent<DoSkill>().BeforeSkill();
        currentcooldown = 0;
        skillavaliable = false;
        Fix64Vector2 Suiteplace = singplace - sn * (Fix64)2;
        GameObject MySuite = Instantiate(MySuiteObj, Suiteplace.ToV2(), Quaternion.identity);
        MySuite.GetComponent<SilenceSuiteScript>().work(sn * realdistance);
    }

    void SkillY2bSetLevel(int i)
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
