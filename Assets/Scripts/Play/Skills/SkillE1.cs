using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillE1 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject TheRock;
    public float maxdistance = 5;
    public float damage = 10;
    public float bombforce = 8;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start ()
    {
        currentcooldown = cooldowntime;
    }
	
    // Update is called once per frame
    void GoSkillE1()
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
        GetComponent<DoSkill>().BeforeSkill();
        Fix64 mdf = (Fix64)maxdistance;
        if (skilldirection.LengthSquare() > mdf * mdf)
            actionplace = singplace + skilldirection.normalized() * mdf;
        GameObject MyRock = Instantiate(TheRock, actionplace.ToV2(), Quaternion.identity);
        MyRock.GetComponent<RockExplode>().damage = damage;
        MyRock.GetComponent<RockExplode>().bombforce = bombforce;
        currentcooldown = 0;
        skillavaliable = false;
    }

    void SkillE1SetLevel(int i)
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
